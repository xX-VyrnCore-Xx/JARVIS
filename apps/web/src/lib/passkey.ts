// Passkey dei dispositivi associati (ADR-0042): registrazione e approvazione delle conferme con WebAuthn.

import { ApiError, DEVICE_MODE, post } from "./api";

const b64url = (buf: ArrayBuffer) =>
  btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const fromB64url = (s: string) =>
  Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4)), (c) => c.charCodeAt(0));

/** Passkey utilizzabili: dispositivo associato, contesto sicuro (HTTPS) e WebAuthn disponibile. */
export const PASSKEY_SUPPORTED =
  DEVICE_MODE && typeof window !== "undefined" && window.isSecureContext && "PublicKeyCredential" in window;

interface Descriptor {
  id: string;
}

/** Registra una passkey per questo dispositivo (impronta, volto o PIN del telefono). */
export async function registerPasskey(): Promise<void> {
  const o = await post<any>("/api/passkeys/register/options");
  const cred = (await navigator.credentials.create({
    publicKey: {
      challenge: fromB64url(o.challenge),
      rp: o.rp,
      user: { ...o.user, id: fromB64url(o.user.id) },
      pubKeyCredParams: o.pubKeyCredParams,
      authenticatorSelection: o.authenticatorSelection,
      excludeCredentials: (o.excludeCredentials as Descriptor[]).map((c) => ({ type: "public-key" as const, id: fromB64url(c.id) })),
      attestation: "none",
      timeout: o.timeout,
    },
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error("registrazione della passkey annullata");
  const r = cred.response as AuthenticatorAttestationResponse;
  await post("/api/passkeys/register", { id: cred.id, clientDataJSON: b64url(r.clientDataJSON), attestationObject: b64url(r.attestationObject) });
}

/** Decide una conferma; se il Core chiede la passkey, la usa e ripete la richiesta. */
export async function decideConfirmation<T = any>(id: string, approve: boolean): Promise<T> {
  const path = `/api/confirmations/${encodeURIComponent(id)}`;
  try {
    return await post<T>(path, { approve });
  } catch (e) {
    if (!(e instanceof ApiError) || e.code !== "passkey_required" || !PASSKEY_SUPPORTED) throw e;
  }
  const o = await post<any>("/api/passkeys/assert/options", { confirmation_id: id });
  const cred = (await navigator.credentials.get({
    publicKey: {
      challenge: fromB64url(o.challenge),
      rpId: o.rpId,
      userVerification: "required",
      timeout: o.timeout,
      allowCredentials: (o.allowCredentials as Descriptor[]).map((c) => ({ type: "public-key" as const, id: fromB64url(c.id) })),
    },
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error("conferma con la passkey annullata");
  const r = cred.response as AuthenticatorAssertionResponse;
  return post<T>(path, {
    approve,
    passkey: { id: cred.id, clientDataJSON: b64url(r.clientDataJSON), authenticatorData: b64url(r.authenticatorData), signature: b64url(r.signature) },
  });
}
