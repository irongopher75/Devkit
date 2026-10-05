import { useCallback, useEffect, useState } from "react";
import {
  CommandView,
  connectEvents,
  fetchState,
  restartCommand,
  runCommand,
  stopCommand,
} from "./api";
import { CommandButton } from "./components/CommandButton";
import { LogPanel } from "./components/LogPanel";

const MAX_LINES_PER_COMMAND = 2000;

export function App() {
  const [commands, setCommands] = useState<CommandView[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    fetchState()
      .then((res) => {
        setCommands(res.commands);
        if (res.commands.length > 0) setSelected(res.commands[0].id);
      })
      .catch((err) => console.error("failed to load state", err));
  }, []);

  useEffect(() => {
    const close = connectEvents((event) => {
      setConnected(true);
      if (event.type === "process_state") {
        setCommands((prev) =>
          prev.map((c) =>
            c.id === event.command_id
              ? { ...c, status: { state: event.state, pid: event.pid, exit_code: event.exit_code } }
              : c,
          ),
        );
      } else if (event.type === "log_line") {
        setLogs((prev) => {
          const existing = prev[event.command_id] ?? [];
          const next = [...existing, event.line];
          if (next.length > MAX_LINES_PER_COMMAND) next.splice(0, next.length - MAX_LINES_PER_COMMAND);
          return { ...prev, [event.command_id]: next };
        });
      }
      // process_stats (cpu/mem/ports) lands here too once the status graph
      // in a later milestone wants it; the log/button view doesn't need it.
    });
    return close;
  }, []);

  const act = useCallback(
    (id: string, fn: (id: string) => Promise<void>) => {
      fn(id).catch((err) => console.error(err));
    },
    [],
  );

  const selectedLines = selected ? logs[selected] ?? [] : [];

  return (
    <div className="app">
      <header className="app-header">
        <h1>devkit</h1>
        <span className={`conn-pill ${connected ? "up" : "down"}`}>
          {connected ? "connected" : "connecting…"}
        </span>
      </header>
      <div className="app-body">
        <div className="command-list">
          {commands.length === 0 && <p className="empty">No commands detected for this project.</p>}
          {commands.map((c) => (
            <CommandButton
              key={c.id}
              command={c}
              selected={c.id === selected}
              onSelect={() => setSelected(c.id)}
              onRun={() => act(c.id, runCommand)}
              onStop={() => act(c.id, stopCommand)}
              onRestart={() => act(c.id, restartCommand)}
            />
          ))}
        </div>
        <div className="log-area">
          <div className="log-area-title">{selected ? `logs: ${selected}` : "logs"}</div>
          <LogPanel commandId={selected} lines={selectedLines} />
        </div>
      </div>
    </div>
  );
}
