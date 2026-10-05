const TOKEN =
  (document.querySelector('meta[name="devkit-token"]') as HTMLMetaElement | null)?.content ?? "";

export type ProcessState = "stopped" | "starting" | "running" | "stopping" | "crashed";

export interface ProcessStatus {
  state: ProcessState;
  pid: number | null;
  exit_code: number | null;
}

export interface CommandView {
  id: string;
  name: string;
  cmd: string;
  source: string;
  auto_start: boolean;
  status: ProcessStatus;
}

export interface StateResponse {
  commands: CommandView[];
}

export type DevkitEvent =
  | {
      type: "process_state";
      command_id: string;
      state: ProcessState;
      pid: number | null;
      exit_code: number | null;
      ts: string;
    }
  | {
      type: "process_stats";
      command_id: string;
      cpu_percent: number;
      mem_bytes: number;
      ports: number[];
      ts: string;
    }
  | {
      type: "log_line";
      command_id: string;
      stream: "pty" | "system";
      line: string;
      ts: string;
    };

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { ...(init?.headers ?? {}), "x-devkit-token": TOKEN },
  });
  if (!res.ok) {
    throw new Error(`${path} -> ${res.status} ${await res.text()}`);
  }
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

export const fetchState = () => call<StateResponse>("/api/state");
export const runCommand = (id: string) => call<void>(`/api/commands/${id}/run`, { method: "POST" });
export const stopCommand = (id: string) => call<void>(`/api/commands/${id}/stop`, { method: "POST" });
export const restartCommand = (id: string) =>
  call<void>(`/api/commands/${id}/restart`, { method: "POST" });

/**
 * Opens the event stream. The daemon replays its ring buffer first, so a
 * freshly opened socket sees recent history before switching to live
 * updates — no separate "catch up" request needed.
 */
export function connectEvents(onEvent: (event: DevkitEvent) => void): () => void {
  const url = new URL("/ws", window.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("token", TOKEN);

  const socket = new WebSocket(url);
  socket.onmessage = (ev) => {
    try {
      onEvent(JSON.parse(ev.data) as DevkitEvent);
    } catch {
      // malformed frame; drop it rather than crash the UI
    }
  };

  return () => socket.close();
}
