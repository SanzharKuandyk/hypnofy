import { open, save } from "@tauri-apps/plugin-dialog";

export async function chooseScript(): Promise<string | null> {
  return open({
    multiple: false,
    filters: [{ name: "Scripts", extensions: ["txt", "md", "json"] }],
  });
}

export async function chooseModelFolder(): Promise<string | null> {
  return open({ directory: true, multiple: false });
}

export async function chooseGguf(): Promise<string | null> {
  return open({ multiple: false, filters: [{ name: "GGUF models", extensions: ["gguf"] }] });
}

export async function chooseExport(title: string, audio = false): Promise<string | null> {
  return save({
    defaultPath: title.replace(/[<>:"/\\|?*]/g, "-") + (audio ? ".wav" : ".txt"),
    filters: audio
      ? [{ name: "WAV audio", extensions: ["wav"] }]
      : [
          { name: "Plain text", extensions: ["txt"] },
          { name: "Markdown", extensions: ["md"] },
          { name: "Hypnofy script", extensions: ["json"] },
        ],
  });
}

export async function chooseAmbience(): Promise<string | null> {
  return open({
    multiple: false,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "flac", "ogg"] }],
  });
}

export async function chooseRecoveryExport(title: string): Promise<string | null> {
  return save({
    defaultPath: title.replace(/[<>:"/\\|?*]/g, "-") + ".json",
    filters: [{ name: "Original Hypnofy script", extensions: ["json"] }],
  });
}

export async function chooseExportFolder(): Promise<string | null> {
  return open({
    directory: true,
    multiple: false,
    title: "Choose a folder for the exported scripts",
  });
}
