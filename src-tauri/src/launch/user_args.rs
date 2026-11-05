//! User-supplied ("custom") JVM arguments.
//!
//! Stored per instance as the raw string the user typed (so quoting and
//! line breaks survive a round trip through the editor), then parsed and
//! validated here -- both when saving and again at launch, so a hand-edited
//! `instances.json` can never smuggle something unvalidated into the
//! command line.
//!
//! Three jobs:
//!
//! 1. `parse` -- shell-style tokenizing (whitespace-separated, single or
//!    double quotes group a token) plus validation.
//! 2. `strip_overridden` -- remove launcher defaults the user is
//!    replacing, so e.g. `-XX:+UseZGC` doesn't collide with Blocklight's
//!    own `-XX:+UseG1GC` (the JVM aborts on two collector selections).
//! 3. The caller appends the parsed args after everything else but
//!    before the main class, so for flags where the JVM lets the last
//!    occurrence win, the user's value wins.

/// Flags that Blocklight (or the loader's version JSON) owns. Letting
/// users override these doesn't customize the launch, it breaks it: the
/// classpath and module path are computed from the prepared libraries,
/// and the natives directory is wherever Blocklight extracted LWJGL.
const BLOCKED_EXACT: &[&str] = &["-cp", "-classpath", "--class-path", "-p", "--module-path", "-jar"];
const BLOCKED_PREFIXES: &[&str] = &[
    "--class-path=",
    "--module-path=",
    "-Djava.library.path",
    "-Dorg.lwjgl.librarypath",
];

/// Collector-selection flags. The JVM refuses to start if two of these
/// are present ("Conflicting collector combinations").
const COLLECTOR_FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+UseZGC",
    "-XX:+UseShenandoahGC",
    "-XX:+UseParallelGC",
    "-XX:+UseSerialGC",
    "-XX:+UseConcMarkSweepGC",
];

/// Splits `raw` into tokens and validates them. Returns a human-readable
/// reason on failure (the caller wraps it in an `AppError`).
///
/// Quoting rules are deliberately minimal: `"` and `'` group text
/// (including whitespace) into one token and are removed. There is no
/// backslash escaping -- Windows paths are full of backslashes, and
/// treating them as escapes would silently corrupt them.
pub fn parse(raw: &str) -> Result<Vec<String>, String> {
    let tokens = tokenize(raw)?;
    validate(&tokens)?;
    Ok(tokens)
}

fn tokenize(raw: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;

    for ch in raw.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                    in_token = true; // `""` is a (empty) token, not nothing
                } else if ch.is_whitespace() {
                    if in_token {
                        tokens.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                } else {
                    current.push(ch);
                    in_token = true;
                }
            }
        }
    }

    if quote.is_some() {
        return Err("unterminated quote".to_string());
    }
    if in_token {
        tokens.push(current);
    }
    Ok(tokens)
}

fn validate(tokens: &[String]) -> Result<(), String> {
    for (i, tok) in tokens.iter().enumerate() {
        if tok.contains('\0') {
            return Err("contains an invalid character".to_string());
        }

        if BLOCKED_EXACT.contains(&tok.as_str())
            || BLOCKED_PREFIXES.iter().any(|p| tok.starts_with(p))
        {
            return Err(format!(
                "`{tok}` can't be set here -- Blocklight manages the classpath, module path \
                 and native libraries itself"
            ));
        }

        if !tok.starts_with('-') {
            // A bare word is only legitimate as the separate value of a
            // preceding `--long-option` (e.g. `--add-opens java.base/...`).
            // Anything else is almost certainly a pasted `java` command,
            // a main class, or a game argument -- none of which belong in
            // the JVM options.
            let is_value = i > 0 && {
                let prev = &tokens[i - 1];
                prev.starts_with("--") && !prev.contains('=')
            };
            if !is_value {
                return Err(if i == 0 {
                    format!(
                        "`{tok}` isn't a JVM option -- arguments should start with `-` \
                         (don't include the `java` command itself)"
                    )
                } else {
                    format!("unexpected `{tok}` -- JVM options start with `-`")
                });
            }
        }
    }
    Ok(())
}

/// Identity of a flag for override purposes: two args with the same key
/// set the same thing. `None` for non-flags (option values).
fn flag_key(arg: &str) -> Option<String> {
    if !arg.starts_with('-') {
        return None;
    }
    for heap in ["-Xmx", "-Xms", "-Xss", "-Xmn"] {
        if arg.starts_with(heap) {
            return Some(heap.to_string());
        }
    }
    if let Some(rest) = arg.strip_prefix("-XX:") {
        let rest = rest.trim_start_matches(['+', '-']);
        let name = rest.split('=').next().unwrap_or(rest);
        return Some(format!("-XX:{name}"));
    }
    if let Some(rest) = arg.strip_prefix("-D") {
        let name = rest.split('=').next().unwrap_or(rest);
        return Some(format!("-D{name}"));
    }
    Some(arg.to_string())
}

fn is_collector_flag(arg: &str) -> bool {
    COLLECTOR_FLAGS.contains(&arg)
}

/// Removes from `defaults` (Blocklight's own flags, *before* the version
/// JSON's args are added) everything the user is overriding.
pub fn strip_overridden(defaults: &mut Vec<String>, user: &[String]) {
    if user.is_empty() {
        return;
    }

    let user_keys: Vec<String> = user.iter().filter_map(|a| flag_key(a)).collect();

    // Picking a different garbage collector means the G1-specific tuning
    // no longer applies either.
    let user_picks_other_collector = user
        .iter()
        .any(|a| is_collector_flag(a) && a != "-XX:+UseG1GC");

    // Setting a max heap without a matching initial heap would otherwise
    // leave Blocklight's `-Xms` (sized for the *old* max) in place, and a
    // JVM with Xms > Xmx refuses to start.
    let user_sets_xmx = user_keys.iter().any(|k| k == "-Xmx");

    defaults.retain(|d| {
        let key = match flag_key(d) {
            Some(k) => k,
            None => return true,
        };
        if user_keys.contains(&key) {
            return false;
        }
        if user_sets_xmx && key == "-Xms" {
            return false;
        }
        if user_picks_other_collector {
            if is_collector_flag(d) {
                return false;
            }
            let name = key.trim_start_matches("-XX:");
            if key.starts_with("-XX:")
                && (name.starts_with("G1") || name == "InitiatingHeapOccupancyPercent")
            {
                return false;
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_on_whitespace_and_newlines() {
        assert_eq!(
            parse("-Xmx6G  -XX:+UseZGC\n-Dfoo=bar").unwrap(),
            v(&["-Xmx6G", "-XX:+UseZGC", "-Dfoo=bar"])
        );
    }

    #[test]
    fn empty_and_blank_input_is_no_args() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("  \n\t ").unwrap().is_empty());
    }

    #[test]
    fn quotes_group_and_are_removed() {
        assert_eq!(
            parse(r#"-Dpath="/my dir/x" -Dother='a b'"#).unwrap(),
            v(&["-Dpath=/my dir/x", "-Dother=a b"])
        );
    }

    #[test]
    fn backslashes_are_literal_for_windows_paths() {
        assert_eq!(
            parse(r#"-javaagent:"C:\tools\agent.jar""#).unwrap(),
            v(&[r"-javaagent:C:\tools\agent.jar"])
        );
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        assert!(parse(r#"-Dfoo="bar"#).unwrap_err().contains("unterminated"));
    }

    #[test]
    fn blocks_classpath_module_path_and_natives() {
        for bad in [
            "-cp x",
            "-classpath x",
            "--class-path=x",
            "-p x",
            "--module-path=x",
            "-jar x.jar",
            "-Djava.library.path=/tmp",
        ] {
            assert!(parse(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn rejects_pasted_java_command_and_stray_words() {
        assert!(parse("java -Xmx4G").unwrap_err().contains("java"));
        assert!(parse("-Xmx4G net.minecraft.client.main.Main").is_err());
    }

    #[test]
    fn allows_separate_values_for_long_options() {
        assert_eq!(
            parse("--add-opens java.base/java.lang=ALL-UNNAMED").unwrap(),
            v(&["--add-opens", "java.base/java.lang=ALL-UNNAMED"])
        );
    }

    #[test]
    fn user_xmx_replaces_both_default_heap_flags() {
        let mut d = v(&["-Xms4096M", "-Xmx4096M", "-XX:+UseG1GC"]);
        strip_overridden(&mut d, &v(&["-Xmx2G"]));
        assert_eq!(d, v(&["-XX:+UseG1GC"]));
    }

    #[test]
    fn user_collector_replaces_g1_and_its_tuning() {
        let mut d = v(&[
            "-XX:+UseG1GC",
            "-XX:G1HeapRegionSize=8M",
            "-XX:G1ReservePercent=20",
            "-XX:InitiatingHeapOccupancyPercent=15",
            "-XX:+DisableExplicitGC",
        ]);
        strip_overridden(&mut d, &v(&["-XX:+UseZGC"]));
        assert_eq!(d, v(&["-XX:+DisableExplicitGC"]));
    }

    #[test]
    fn user_choosing_g1_keeps_tuning_but_dedupes() {
        let mut d = v(&["-XX:+UseG1GC", "-XX:G1HeapRegionSize=8M"]);
        strip_overridden(&mut d, &v(&["-XX:+UseG1GC"]));
        assert_eq!(d, v(&["-XX:G1HeapRegionSize=8M"]));
    }

    #[test]
    fn user_can_turn_off_a_default_flag() {
        let mut d = v(&["-XX:+AlwaysPreTouch", "-XX:+DisableExplicitGC"]);
        strip_overridden(&mut d, &v(&["-XX:-AlwaysPreTouch"]));
        assert_eq!(d, v(&["-XX:+DisableExplicitGC"]));
    }

    #[test]
    fn system_properties_override_by_name() {
        let mut d = v(&["-Dfml.earlyprogresswindow=false", "-Dminecraft.launcher.brand=Blocklight"]);
        strip_overridden(&mut d, &v(&["-Dfml.earlyprogresswindow=true"]));
        assert_eq!(d, v(&["-Dminecraft.launcher.brand=Blocklight"]));
    }
}
