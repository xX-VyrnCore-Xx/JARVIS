export const ACTIVITY_LABELS = {
  general: "Conversazione", coding: "Sviluppo", research: "Ricerca",
  automation: "Automazioni", security: "Sicurezza", system: "Sistema", files: "File e memoria",
} as const;

type Category = keyof typeof ACTIVITY_LABELS;
export type AvatarActivity = { category: Category; outcome?: "attention" | "error" };

export function activityFromPrompt(prompt: string): AvatarActivity {
  const rules: [Category, RegExp][] = [
    ["security", /\b(sicurezza|audit|cybersecurity|hardening|backup)\b/i],
    ["automation", /\b(automazion[ei]|n8n|pianifica|ogni giorno|ogni ora|promemoria)\b/i],
    ["coding", /\b(python|javascript|typescript|codice|programma|programmare|test unitari|script|git|refactoring|compila|debug|sandbox)\b/i],
    ["system", /\b(sistema|dischi|cpu|ram|processi|terminale|nodo|calcolatrice|monitoraggio attività|gestione attività)\b/i],
    ["files", /\b(file|cartell[ae]|workspace|memoria|ricorda|ricordi|leggi|preferenze)\b/i],
    ["research", /\b(cerca|ricerca|fonti|wikipedia|naviga|notizie)\b|https?:\/\//i],
  ];
  return { category: rules.find(([, pattern]) => pattern.test(prompt))?.[0] || "general" };
}

const toolCategories: Record<string, Category> = {
  code: "coding", web: "research", weather: "research", workflow: "automation",
  n8n: "automation", todo: "automation", security: "security", system: "system",
  pc: "system", process: "system", shell: "system", node: "system", fs: "files", memory: "files", notes: "files",
};

export function activityFromResult(
  initial: AvatarActivity,
  steps: { kind: string; tool?: string; agent?: string }[],
  outcome?: AvatarActivity["outcome"],
): AvatarActivity {
  const tool = steps.find((step) => step.kind === "tool_call" && step.tool)?.tool;
  const agent = steps.find((step) => step.kind === "delegation")?.agent;
  const toolCategory = tool && toolCategories[tool.split(".")[0]];
  const category = agent === "monitoring" ? "security"
    : agent === "coding" ? "coding"
    : toolCategory || initial.category;
  return { category, outcome };
}
