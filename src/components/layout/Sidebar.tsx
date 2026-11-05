import { NavLink } from "react-router-dom";
import type { AccountState, ConnectivityState } from "@/lib/types";
import { OfflineStatusCard } from "../offline/OfflineStatusCard";

const NAV_ITEMS = [
  { to: "/", label: "Home", icon: HomeIcon },
  { to: "/mods", label: "Mods", icon: BlockIcon },
  { to: "/resource-packs", label: "Resource Packs", icon: PaletteIcon },
  { to: "/shaders", label: "Shaders", icon: SparkleIcon },
  { to: "/settings", label: "Settings", icon: GearIcon },
];

export function Sidebar({
  connectivity,
  account,
  onAccountChange,
}: {
  connectivity: ConnectivityState;
  account: AccountState | null;
  onAccountChange: () => void;
}) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark" />
        <span className="brand-name">Blocklight</span>
      </div>

      <nav className="sidebar-nav">
        {NAV_ITEMS.map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) => `sidebar-link${isActive ? " active" : ""}`}
          >
            <Icon />
            {label}
          </NavLink>
        ))}
      </nav>

      <OfflineStatusCard
        connectivity={connectivity}
        account={account}
        onAccountChange={onAccountChange}
        onOpenSettings={() => {
          window.location.hash = "#/settings";
        }}
      />
    </aside>
  );
}

function iconProps() {
  return {
    className: "sidebar-icon",
    viewBox: "0 0 16 16",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
  } as const;
}

function HomeIcon() {
  return (
    <svg {...iconProps()}>
      <path
        d="M2 7.5 8 2l6 5.5V14a1 1 0 0 1-1 1h-3v-4.5H6V15H3a1 1 0 0 1-1-1V7.5Z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function BlockIcon() {
  return (
    <svg {...iconProps()}>
      <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" stroke="currentColor" strokeWidth="1.3" />
      <path d="M2.5 6h11M6 2.5v11" stroke="currentColor" strokeWidth="1.1" opacity="0.6" />
    </svg>
  );
}

function PaletteIcon() {
  return (
    <svg {...iconProps()}>
      <path
        d="M8 2.5a5.5 5.5 0 1 0 0 11c.7 0 1.2-.6 1.2-1.3 0-.3-.1-.6-.3-.8-.2-.2-.3-.5-.3-.8 0-.7.6-1.2 1.2-1.2h1.4a2.3 2.3 0 0 0 2.3-2.3C13.5 4.6 11 2.5 8 2.5Z"
        stroke="currentColor"
        strokeWidth="1.3"
      />
      <circle cx="5.3" cy="7" r="0.9" fill="currentColor" />
      <circle cx="8" cy="5" r="0.9" fill="currentColor" />
      <circle cx="10.7" cy="7" r="0.9" fill="currentColor" />
    </svg>
  );
}

function SparkleIcon() {
  return (
    <svg {...iconProps()}>
      <path
        d="M8 2.5c.3 2 1.5 3.2 3.5 3.5-2 .3-3.2 1.5-3.5 3.5-.3-2-1.5-3.2-3.5-3.5 2-.3 3.2-1.5 3.5-3.5Z"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinejoin="round"
      />
      <path
        d="M12.2 10.2c.15 1 .75 1.6 1.75 1.75-1 .15-1.6.75-1.75 1.75-.15-1-.75-1.6-1.75-1.75 1-.15 1.6-.75 1.75-1.75Z"
        stroke="currentColor"
        strokeWidth="1.1"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg {...iconProps()}>
      <circle cx="8" cy="8" r="2.1" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M8 2.7v1.4M8 11.9v1.4M13.3 8h-1.4M4.1 8H2.7M11.6 4.4l-1 1M5.4 10.6l-1 1M11.6 11.6l-1-1M5.4 5.4l-1-1"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
      />
    </svg>
  );
}
