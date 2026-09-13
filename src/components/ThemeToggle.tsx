import { Monitor, Moon, Sun } from "lucide-react";
import { useStore, type ThemeMode } from "@/store";
import { useT } from "@/i18n";
import { Button } from "./ui";

const ORDER: ThemeMode[] = ["light", "dark", "system"];

export function ThemeToggle() {
  const { theme, setTheme } = useStore();
  const t = useT();
  const next = ORDER[(ORDER.indexOf(theme) + 1) % ORDER.length];
  const label = (mode: ThemeMode) =>
    mode === "light"
      ? t("themeLight")
      : mode === "dark"
        ? t("themeDark")
        : t("themeSystem");
  const Icon = theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;
  const title = t("themeToggleTitle", {
    current: label(theme),
    next: label(next),
  });
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setTheme(next)}
      title={title}
      aria-label={title}
    >
      <Icon size={16} />
    </Button>
  );
}
