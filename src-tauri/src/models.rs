use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Modrinth,
    CurseForge,
}

impl Platform {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Platform::Modrinth => "Modrinth",
            Platform::CurseForge => "CurseForge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Mod,
    ResourcePack,
    ShaderPack,
    Modpack,
}

impl ContentType {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            ContentType::Mod => "Mod",
            ContentType::ResourcePack => "Resource Pack",
            ContentType::ShaderPack => "Shader Pack",
            ContentType::Modpack => "Modpack",
        }
    }

    pub fn install_subdir(&self) -> &'static str {
        match self {
            ContentType::Mod => "mods",
            ContentType::ResourcePack => "resourcepacks",
            ContentType::ShaderPack => "shaderpacks",
            ContentType::Modpack => ".",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    Forge,
    NeoForge,
    Quilt,
    Minecraft,
}

impl Loader {
    pub fn parse(raw: &str) -> Loader {
        match raw.to_ascii_lowercase().as_str() {
            "fabric" => Loader::Fabric,
            "forge" => Loader::Forge,
            "neoforge" => Loader::NeoForge,
            "quilt" => Loader::Quilt,
            _ => Loader::Minecraft,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Loader::Fabric => "Fabric",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
            Loader::Quilt => "Quilt",
            Loader::Minecraft => "Minecraft",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub java_major_version: Option<u32>,
    pub instance_dir: String,
    pub installed: Vec<InstalledContent>,
    #[serde(default)]
    pub custom_jvm_arguments: Vec<String>,
}
