// Icone vettoriali in linea (tratto 1,6, ereditano il colore): nessuna dipendenza e nessuna richiesta esterna.

const PATHS: Record<string, string> = {
  dashboard: "M12 3a9 9 0 1 0 9 9M12 7a5 5 0 1 0 5 5M12 11a1 1 0 1 0 1 1M21 3l-7.5 7.5",
  chat: "M4 5h16v10H9l-5 4zM8 9h8M8 12h5",
  tasks: "M9 6h11M9 12h11M9 18h11M4 6l1 1 2-2M4 12l1 1 2-2M4 18l1 1 2-2",
  automations: "M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4M12 8v4l3 2",
  memory: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5",
  skills: "M12 3l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.4 6.8 19.1l1-5.8-4.3-4.1 5.9-.9z",
  workspace: "M3 7h6l2 2h10v10H3zM3 7V5h6l2 2",
  connectors: "M7 7h4M13 17h4M9 7a4 4 0 0 1 0 8H7M15 9h2a4 4 0 0 1 0 8M7 12h10",
  devices: "M4 5h12v9H4zM2 18h16M18 9h4v10h-4z",
  audit: "M12 3l7 3v6c0 4-3 7.5-7 9-4-1.5-7-5-7-9V6zM9 12l2 2 4-4",
  system: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19 12l2-1-1-3-2 .5-1.5-1.5.5-2-3-1-1 2h-2l-1-2-3 1 .5 2L6 7.5 4 7 3 10l2 1v2l-2 1 1 3 2-.5 1.5 1.5-.5 2 3 1 1-2h2l1 2 3-1-.5-2 1.5-1.5 2 .5 1-3-2-1z",
  bell: "M6 16V11a6 6 0 1 1 12 0v5l2 2H4zM10 20a2 2 0 0 0 4 0",
  theme: "M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z",
  search: "M11 18a7 7 0 1 1 0-14 7 7 0 0 1 0 14zM20 20l-4-4",
  mic: "M12 15a3 3 0 0 0 3-3V6a3 3 0 1 0-6 0v6a3 3 0 0 0 3 3zM6 11a6 6 0 0 0 12 0M12 17v4",
  send: "M4 12l16-8-6 16-2-7z",
  stop: "M7 7h10v10H7z",
  collapse: "M15 6l-6 6 6 6",
  expand: "M9 6l6 6-6 6",
  check: "M5 12l4 4 10-10",
  alert: "M12 4l9 16H3zM12 10v4M12 17v.5",
  info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8v.5",
  bolt: "M13 3L5 13h6l-1 8 8-10h-6z",
  clock: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2",
  spark: "M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5L18 18M6 18l2.5-2.5M15.5 8.5L18 6",
  node: "M5 5h14v6H5zM5 13h14v6H5zM8 8h.01M8 16h.01",
  shield: "M12 3l7 3v6c0 4-3 7.5-7 9-4-1.5-7-5-7-9V6z",
  gauge: "M4 16a8 8 0 1 1 16 0M12 16l4-5",
  close: "M6 6l12 12M18 6L6 18",
};

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, className = "" }: { name: string; size?: number; className?: string }) {
  return (
    <svg className={`icon ${className}`} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      <path d={PATHS[name] || PATHS.info} />
    </svg>
  );
}
