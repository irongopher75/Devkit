import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

interface Props {
  commandId: string | null;
  lines: string[];
}

export function LogPanel({ commandId, lines }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const writtenRef = useRef(0);
  const currentCommandRef = useRef<string | null>(null);

  useEffect(() => {
    const term = new Terminal({
      convertEol: true,
      fontSize: 13,
      theme: { background: "#0b0e14" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    if (containerRef.current) {
      term.open(containerRef.current);
      fit.fit();
    }
    termRef.current = term;
    fitRef.current = fit;

    const onResize = () => fit.fit();
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      term.dispose();
    };
  }, []);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;

    if (currentCommandRef.current !== commandId) {
      term.reset();
      writtenRef.current = 0;
      currentCommandRef.current = commandId;
    }

    for (const line of lines.slice(writtenRef.current)) {
      term.writeln(line);
    }
    writtenRef.current = lines.length;
  }, [commandId, lines]);

  return <div className="log-panel" ref={containerRef} />;
}
