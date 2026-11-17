// E2E dell'interfaccia: chat, strumento a basso rischio, conferma di scrittura, memoria e audit.
// Uso: JARVIS_URL=http://localhost:8765 node tests/e2e/ui_smoke.cjs [cartella-screenshot]
// Richiede un Core appena avviato con JARVIS_AI_BACKEND=offline (risposte deterministiche) e una
// JARVIS_DATA_DIR vuota (i contatori presuppongono uno stato iniziale pulito), più il pacchetto `playwright`.
const { chromium } = require("playwright");

const URL = process.env.JARVIS_URL || "http://localhost:8765";
const shots = process.argv[2];

function check(cond, msg) {
  if (!cond) throw new Error("Verifica fallita: " + msg);
  console.log("✓ " + msg);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 }, deviceScaleFactor: shots ? 2 : 1 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(URL);

  const send = async (text) => {
    const before = await page.locator(".msg.assistant").count();
    await page.fill("#input", text);
    await page.keyboard.press("Enter");
    await page.waitForFunction((n) => document.querySelectorAll(".msg.assistant").length > n, before, { timeout: 30000 });
    return (await page.locator(".msg.assistant").last().innerText()).trim();
  };

  check((await page.locator("#st-ai").innerText()).includes("offline"), "stato AI visibile nell'intestazione");
  check((await send("ciao")).includes("JARVIS"), "risposta al saluto");
  check((await send("che ore sono")).startsWith("Sono le"), "strumento time.now eseguito senza conferma");

  await send("scrivi diario.txt: nota creata dal test E2E");
  await page.waitForSelector(".confirm button.primary");
  check((await page.locator(".confirm").last().innerText()).includes("diario.txt"), "scheda di conferma con effetto atteso");
  // Notifiche (RF-13): la conferma in attesa compare come notifica e toast, e si chiude quando viene risolta.
  await page.waitForFunction(() => document.querySelector("#notif-count").textContent === "1" && !document.querySelector("#notif-count").hidden);
  check(true, "contatore notifiche: 1 conferma in attesa");
  await page.waitForSelector(".toast.warning");
  check((await page.locator(".toast.warning").first().innerText()).includes("Conferma richiesta"), "toast della conferma in attesa");
  await page.click("#bell");
  await page.waitForSelector("#notif-list .notif.unread");
  check((await page.locator("#notif-list").innerText()).includes("fs.write"), "notifica elencata nel pannello con il task");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-notifiche.png` });
  await page.keyboard.press("Escape");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-conferma.png` });
  const before = await page.locator(".msg.assistant").count();
  await page.click(".confirm button.primary");
  await page.waitForFunction((n) => document.querySelectorAll(".msg.assistant").length > n, before);
  check((await page.locator(".msg.assistant").last().innerText()).includes("diario.txt"), "scrittura eseguita dopo l'approvazione");
  await page.waitForFunction(() => document.querySelector("#notif-count").hidden);
  check(true, "notifica chiusa dopo l'approvazione");
  check((await send("leggi diario.txt")).includes("nota creata dal test E2E"), "lettura del file scritto");
  check((await send("leggi ../../etc/passwd")).includes("negato"), "uscita dalla workspace negata");
  await send("ricorda che il test E2E è passato");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-chat.png` });

  await page.click('[data-view="memory"]');
  await page.waitForSelector("#memory .card");
  check((await page.locator("#memory").innerText()).includes("il test E2E è passato"), "memoria visibile nella sezione Memoria");

  await page.click('[data-view="tasks"]');
  await page.waitForSelector("#tasks .card");
  check((await page.locator("#tasks .badge").first().innerText()) === "succeeded", "attività con stato verificato");

  // Automazioni (RF-07): creazione dalla chat con conferma, esecuzione manuale, cronologia.
  await page.click('[data-view="chat"]');
  await send("ogni ora info sistema");
  await page.waitForSelector(".confirm button.primary");
  check((await page.locator(".confirm").last().innerText()).includes("workflow.create"), "creare un'automazione chiede conferma");
  const n = await page.locator(".msg.assistant").count();
  await page.click(".confirm button.primary");
  await page.waitForFunction((k) => document.querySelectorAll(".msg.assistant").length > k, n);
  check((await page.locator(".msg.assistant").last().innerText()).includes("Automazione creata"), "automazione creata dopo l'approvazione");
  await page.click('[data-view="automations"]');
  await page.waitForSelector("#automations .card");
  check((await page.locator("#automations .card").first().innerText()).includes("ogni ora"), "automazione elencata con la frequenza");
  await page.locator("#automations .card").first().getByRole("button", { name: "Esegui ora" }).click();
  await page.waitForFunction(() => document.querySelector("#automations .card .sub:nth-of-type(3)") && /ultima: succeeded/.test(document.querySelector("#automations").innerText));
  await page.locator("#automations .card").first().getByRole("button", { name: "Cronologia" }).click();
  await page.waitForFunction(() => /manual · succeeded/.test(document.querySelector("#automations .runs").innerText));
  check(true, "esecuzione manuale registrata nella cronologia");
  // Creazione dal modulo dell'interfaccia.
  await page.click("#wf-new summary");
  await page.fill('#wf-form input[name="name"]', "Battito dal modulo");
  await page.fill('#wf-form input[name="every"]', "5");
  await page.selectOption("#wf-steps select", "fs.write");
  await page.fill('#wf-steps [data-arg="path"]', "battito.txt");
  await page.fill('#wf-steps [data-arg="content"]', "vivo");
  page.once("dialog", (d) => d.accept());
  await page.click('#wf-form button[type="submit"]');
  await page.waitForFunction(() => /Battito dal modulo/.test(document.querySelector("#automations").innerText));
  check((await page.locator("#automations").innerText()).includes("ogni 5 minuti"), "automazione creata dal modulo");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-automazioni.png` });

  await page.click('[data-view="audit"]');
  await page.waitForFunction(() => document.querySelector("#audit-summary").textContent.includes("integra"));
  check(true, "catena di audit integra");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-audit.png` });

  await page.setViewportSize({ width: 390, height: 800 });
  await page.click('[data-view="chat"]');
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  check(!overflow, "nessuno scorrimento orizzontale a 390 px");
  if (shots) await page.screenshot({ path: `${shots}/jarvis-mobile.png` });

  check(errors.length === 0, "nessun errore JavaScript o CSP in console" + (errors.length ? ": " + errors.join(" | ") : ""));
  await browser.close();
})().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
