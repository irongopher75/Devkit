import type { CommandView } from "../api";

const STATE_COLOR: Record<string, string> = {
  stopped: "#6b7280",
  starting: "#d97706",
  running: "#16a34a",
  stopping: "#d97706",
  crashed: "#dc2626",
};

interface Props {
  command: CommandView;
  selected: boolean;
  onSelect: () => void;
  onRun: () => void;
  onStop: () => void;
  onRestart: () => void;
}

export function CommandButton({ command, selected, onSelect, onRun, onStop, onRestart }: Props) {
  const { state, pid } = command.status;
  const isRunning = state === "running" || state === "starting";

  return (
    <div className={`command-card${selected ? " selected" : ""}`} onClick={onSelect}>
      <div className="command-card-header">
        <span className="state-dot" style={{ background: STATE_COLOR[state] }} />
        <span className="command-name">{command.name}</span>
        <span className="command-source">{command.source}</span>
      </div>
      <div className="command-meta">
        {state}
        {pid ? ` · pid ${pid}` : ""}
      </div>
      <div className="command-actions" onClick={(e) => e.stopPropagation()}>
        <button disabled={isRunning} onClick={onRun}>
          Run
        </button>
        <button disabled={!isRunning} onClick={onStop}>
          Stop
        </button>
        <button disabled={!isRunning} onClick={onRestart}>
          Restart
        </button>
      </div>
    </div>
  );
}
