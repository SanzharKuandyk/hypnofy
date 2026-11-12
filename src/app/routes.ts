export type LibraryView = "all" | "favorites" | "trash";

export type SettingsSection = "appearance" | "providers" | "voice" | "storage" | "help";

export type Route =
  | { kind: "library"; view: LibraryView; collectionId: string | null }
  | { kind: "script"; id: string; tab?: "narrate" }
  | { kind: "models" }
  | { kind: "sounds"; returnToScriptId?: string; view?: "recordings" }
  | { kind: "settings"; section: SettingsSection };
