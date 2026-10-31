export type LibraryView = "all" | "favorites" | "trash";

export type SettingsSection = "appearance" | "providers" | "voice" | "storage" | "help";

export type Route =
  | { kind: "library"; view: LibraryView; collectionId: string | null }
  | { kind: "script"; id: string }
  | { kind: "models" }
  | { kind: "settings"; section: SettingsSection };
