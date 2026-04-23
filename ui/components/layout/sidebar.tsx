"use client";

import * as React from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  Activity,
  Beaker,
  BookOpen,
  Bot,
  Boxes,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ClipboardCheck,
  Database,
  FileTerminal,
  Frame,
  LogOut,
  MessageCircle,
  Network,
  Plug,
  Radio,
  Route,
  Send,
  Settings,
  Sparkles,
  Timer,
  UserSquare,
  Wrench,
  Zap,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import { logout } from "@/lib/auth";
import { useMotion } from "@/components/ui/motion-safe";
import { BrandMark } from "./brand-mark";

interface NavItem {
  kind?: "item";
  href: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
}

interface NavGroup {
  kind: "group";
  /** Stable id (used for local-storage + keyboard nav). */
  id: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  children: NavItem[];
}

type NavEntry = NavItem | NavGroup;

const ITEMS: NavEntry[] = [
  { href: "/", labelKey: "nav.dashboard", icon: Activity },
  { href: "/plugins", labelKey: "nav.plugins", icon: Boxes },
  { href: "/skills", labelKey: "nav.skills", icon: Wrench },
  { href: "/agents", labelKey: "nav.agents", icon: Bot },
  { href: "/characters", labelKey: "nav.characters", icon: UserSquare },
  { href: "/diary", labelKey: "nav.diary", icon: BookOpen },
  { href: "/rag", labelKey: "nav.rag", icon: Database },
  {
    kind: "group",
    id: "channels",
    labelKey: "nav.channels",
    icon: Radio,
    children: [
      {
        href: "/channels/qq",
        labelKey: "nav.channelQq",
        icon: MessageCircle,
      },
      {
        href: "/channels/telegram",
        labelKey: "nav.channelTelegram",
        icon: Send,
      },
    ],
  },
  { href: "/scheduler", labelKey: "nav.scheduler", icon: Timer },
  { href: "/approvals", labelKey: "nav.approvals", icon: ClipboardCheck },
  { href: "/models", labelKey: "nav.models", icon: Route },
  { href: "/providers", labelKey: "nav.providers", icon: Plug },
  { href: "/embedding", labelKey: "nav.embedding", icon: Sparkles },
  { href: "/tagmemo", labelKey: "nav.tagmemo", icon: Sparkles },
  { href: "/config", labelKey: "nav.config", icon: Settings },
  { href: "/logs", labelKey: "nav.logs", icon: FileTerminal },
  { href: "/hooks", labelKey: "nav.hooks", icon: Zap },
  { href: "/nodes", labelKey: "nav.nodes", icon: Network },
  { href: "/playground/protocol", labelKey: "nav.playground", icon: Beaker },
  { href: "/canvas", labelKey: "nav.canvas", icon: Frame },
];

function isActiveHref(pathname: string, href: string): boolean {
  if (href === "/") return pathname === "/";
  return pathname === href || pathname.startsWith(`${href}/`);
}

const COLLAPSE_KEY = "corlinman.sidebar.collapsed.v1";

function readCollapsed(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}
function writeCollapsed(v: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(COLLAPSE_KEY, v ? "1" : "0");
  } catch {
    /* ignore */
  }
}

interface SidebarProps {
  user?: string;
}

export function Sidebar({ user }: SidebarProps) {
  const pathname = usePathname() ?? "/";
  const router = useRouter();
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = React.useState(false);
  const [hydrated, setHydrated] = React.useState(false);
  const [loggingOut, setLoggingOut] = React.useState(false);

  React.useEffect(() => {
    setCollapsed(readCollapsed());
    setHydrated(true);
  }, []);

  const toggle = () => {
    setCollapsed((prev) => {
      const next = !prev;
      writeCollapsed(next);
      return next;
    });
  };

  async function onLogout() {
    setLoggingOut(true);
    try {
      await logout();
      toast.success(t("auth.logoutSuccess"));
    } catch {
      /* idempotent */
    } finally {
      router.push("/login");
    }
  }

  const width = collapsed && hydrated ? "w-[56px]" : "w-[240px]";

  return (
    <aside
      className={cn(
        "flex shrink-0 flex-col border-r border-border bg-surface/60 transition-[width] duration-200 ease-out",
        width,
      )}
      aria-label={t("nav.dashboard")}
    >
      {/* brand + collapse */}
      <div className="flex h-14 items-center justify-between border-b border-border px-3">
        <Link href="/" className="flex items-center gap-2 overflow-hidden">
          <BrandMarkNudge>
            <BrandMark compact={collapsed && hydrated} />
          </BrandMarkNudge>
        </Link>
        <button
          type="button"
          onClick={toggle}
          aria-label={
            collapsed ? t("nav.expandSidebar") : t("nav.collapseSidebar")
          }
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          {collapsed ? (
            <ChevronsRight className="h-4 w-4" />
          ) : (
            <ChevronsLeft className="h-4 w-4" />
          )}
        </button>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 p-2">
        {ITEMS.map((entry) => {
          if (entry.kind === "group") {
            return (
              <SidebarGroup
                key={entry.id}
                group={entry}
                pathname={pathname}
                collapsed={collapsed && hydrated}
              />
            );
          }
          return (
            <SidebarItem
              key={entry.href}
              item={entry}
              pathname={pathname}
              collapsed={collapsed && hydrated}
            />
          );
        })}
      </nav>

      {/* user chip + footer */}
      <div className="border-t border-border p-3">
        {collapsed && hydrated ? (
          <button
            type="button"
            onClick={onLogout}
            aria-label={t("auth.logoutLabel")}
            disabled={loggingOut}
            className="flex h-8 w-full items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
            data-testid="logout-button"
          >
            <LogOut className="h-4 w-4" />
          </button>
        ) : (
          <div className="flex items-center gap-2">
            <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/15 text-[11px] font-semibold text-primary">
              {(user ?? "a").slice(0, 1).toUpperCase()}
            </div>
            <div className="min-w-0 flex-1 leading-tight">
              <div
                className="truncate text-xs font-medium text-foreground"
                data-testid="nav-user"
              >
                {user ?? "admin"}
              </div>
              <div className="truncate font-mono text-[10px] text-muted-foreground">
                v0.1.1
              </div>
            </div>
            <button
              type="button"
              onClick={onLogout}
              disabled={loggingOut}
              aria-label={t("auth.logoutLabel")}
              className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
              data-testid="logout-button"
            >
              <LogOut className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}

/**
 * Single leaf entry. Extracted so group children can reuse the same visual
 * treatment as top-level items.
 */
function SidebarItem({
  item,
  pathname,
  collapsed,
  nested = false,
  onRef,
  onKeyDown,
}: {
  item: NavItem;
  pathname: string;
  collapsed: boolean;
  nested?: boolean;
  onRef?: (el: HTMLAnchorElement | null) => void;
  onKeyDown?: (e: React.KeyboardEvent<HTMLAnchorElement>) => void;
}) {
  const { t } = useTranslation();
  const active = isActiveHref(pathname, item.href);
  const Icon = item.icon;
  const label = t(item.labelKey);
  return (
    <Link
      ref={onRef}
      href={item.href as never}
      onKeyDown={onKeyDown}
      className={cn(
        "relative flex items-center gap-3 rounded-md px-2.5 py-2 text-sm transition-colors",
        active
          ? "bg-accent/70 text-accent-foreground"
          : "text-muted-foreground hover:bg-accent/40 hover:text-foreground",
        collapsed && "justify-center px-0",
        nested && !collapsed && "pl-8",
      )}
      aria-current={active ? "page" : undefined}
      title={collapsed ? label : undefined}
    >
      {active ? (
        <motion.span
          layoutId="sidebar-indicator"
          className="absolute left-0 top-1 bottom-1 w-[2px] rounded-full bg-primary"
          transition={{
            type: "spring",
            stiffness: 500,
            damping: 40,
            mass: 0.6,
          }}
        />
      ) : null}
      <Icon className="h-4 w-4 shrink-0" />
      {collapsed ? null : <span className="truncate">{label}</span>}
    </Link>
  );
}

/**
 * Collapsible group. Defaults to collapsed; auto-expands when the current
 * route matches one of its children. Keyboard:
 *   - Enter / Space on the toggle flips expanded.
 *   - ArrowDown on the toggle moves focus to the first child.
 *   - ArrowUp on the first child returns focus to the toggle.
 */
function SidebarGroup({
  group,
  pathname,
  collapsed,
}: {
  group: NavGroup;
  pathname: string;
  collapsed: boolean;
}) {
  const { t } = useTranslation();
  const hasActiveChild = group.children.some((c) =>
    isActiveHref(pathname, c.href),
  );
  const [expanded, setExpanded] = React.useState<boolean>(hasActiveChild);

  // Auto-expand whenever the current route matches a child. Closing stays
  // user-driven — we don't force collapse when the route navigates away.
  React.useEffect(() => {
    if (hasActiveChild) setExpanded(true);
  }, [hasActiveChild]);

  const toggleRef = React.useRef<HTMLButtonElement | null>(null);
  const childRefs = React.useRef<Array<HTMLAnchorElement | null>>([]);

  const Icon = group.icon;
  const label = t(group.labelKey);

  const onToggleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      setExpanded((v) => !v);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!expanded) setExpanded(true);
      // Focus is deferred so the child list has a chance to mount.
      requestAnimationFrame(() => childRefs.current[0]?.focus());
    }
  };

  const onChildKeyDown = (
    e: React.KeyboardEvent<HTMLAnchorElement>,
    idx: number,
  ) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      childRefs.current[idx + 1]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (idx === 0) {
        toggleRef.current?.focus();
      } else {
        childRefs.current[idx - 1]?.focus();
      }
    }
  };

  // Collapsed rail: render children as flat icon entries so every channel
  // remains one click away.
  if (collapsed) {
    return (
      <>
        {group.children.map((child) => (
          <SidebarItem
            key={child.href}
            item={child}
            pathname={pathname}
            collapsed
          />
        ))}
      </>
    );
  }

  return (
    <div
      role="group"
      aria-label={label}
      data-testid={`sidebar-group-${group.id}`}
    >
      <button
        ref={toggleRef}
        type="button"
        onClick={() => setExpanded((v) => !v)}
        onKeyDown={onToggleKeyDown}
        aria-expanded={expanded}
        aria-controls={`sidebar-group-${group.id}-list`}
        aria-label={label}
        data-testid={`sidebar-group-toggle-${group.id}`}
        className={cn(
          "relative flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-sm transition-colors",
          hasActiveChild
            ? "font-semibold text-foreground"
            : "text-muted-foreground hover:bg-accent/40 hover:text-foreground",
        )}
      >
        <Icon className="h-4 w-4 shrink-0" />
        <span className="truncate">{label}</span>
        <motion.span
          aria-hidden
          className="ml-auto inline-flex"
          animate={{ rotate: expanded ? 90 : 0 }}
          transition={{ duration: 0.15, ease: "easeOut" }}
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </motion.span>
      </button>
      {expanded ? (
        <ul
          id={`sidebar-group-${group.id}-list`}
          className="mt-0.5 flex flex-col gap-0.5"
          role="list"
        >
          {group.children.map((child, idx) => (
            <li key={child.href}>
              <SidebarItem
                item={child}
                pathname={pathname}
                collapsed={false}
                nested
                onRef={(el) => {
                  childRefs.current[idx] = el;
                }}
                onKeyDown={(e) => onChildKeyDown(e, idx)}
              />
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

/**
 * Plays a 1° rotate + 2% scale nudge on the brand-mark whenever the route
 * changes. Visually tiny but signals "you moved" without competing with the
 * page-transition itself. Disabled under `prefers-reduced-motion`.
 */
function BrandMarkNudge({ children }: { children: React.ReactNode }) {
  const pathname = usePathname() ?? "/";
  const { reduced } = useMotion();
  // Monotonically increasing key drives the animate prop via the pathname.
  // `initial={false}` prevents a nudge on first mount.
  const animate = reduced
    ? { rotate: 0, scale: 1 }
    : { rotate: [0, 1, 0], scale: [1, 1.02, 1] };
  return (
    <motion.span
      key={pathname}
      className="inline-flex origin-center"
      initial={false}
      animate={animate}
      transition={{ duration: 0.3, ease: [0.34, 1.56, 0.64, 1] }}
    >
      {children}
    </motion.span>
  );
}
