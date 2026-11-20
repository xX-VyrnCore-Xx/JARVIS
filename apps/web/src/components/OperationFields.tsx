export interface ParameterSchema {
  type?: string | string[];
  title?: string;
  description?: string;
  format?: string;
  enum?: unknown[];
  const?: unknown;
  properties?: Record<string, ParameterSchema>;
  required?: string[];
  items?: ParameterSchema;
  minLength?: number;
  maxLength?: number;
  minimum?: number;
  maximum?: number;
  minItems?: number;
  maxItems?: number;
}

export type Parameters = Record<string, unknown>;

function fieldType(schema: ParameterSchema) {
  return (Array.isArray(schema.type) ? schema.type.find((type) => type !== "null") : schema.type)
    || (schema.properties ? "object" : schema.items ? "array" : "string");
}

function fields(schema: ParameterSchema) {
  const properties = new Map(Object.entries(schema.properties || {}));
  for (const name of schema.required || []) {
    if (!properties.has(name)) properties.set(name, {});
  }
  return properties;
}

function labelFor(name: string, schema: ParameterSchema) {
  return schema.title || name;
}

export function initialParameters(schema: ParameterSchema): Parameters {
  return Object.fromEntries([...fields(schema)].flatMap<[string, unknown]>(([name, field]) => {
    if (!schema.required?.includes(name) || field.enum || field.const !== undefined) return [];
    const type = fieldType(field);
    if (type === "object") return [[name, initialParameters(field)]];
    if (type === "array") return [[name, []]];
    return [];
  }));
}

export function missingParameters(schema: ParameterSchema, value: unknown, path = ""): string[] {
  if (value === undefined || (typeof value === "number" && !Number.isFinite(value))) return [path];
  if (fieldType(schema) === "object" && value && typeof value === "object" && !Array.isArray(value)) {
    const data = value as Parameters;
    return [...fields(schema)].flatMap(([name, field]) => {
      const present = Object.prototype.hasOwnProperty.call(data, name);
      if (!present && !(schema.required || []).includes(name)) return [];
      const label = labelFor(name, field);
      return missingParameters(field, present ? data[name] : undefined, path ? `${path} › ${label}` : label);
    });
  }
  if (fieldType(schema) === "array" && Array.isArray(value)) {
    if (value.length < (schema.minItems || 0)) return [path];
    return value.flatMap((item, i) => missingParameters(schema.items || {}, item, `${path} ${i + 1}`));
  }
  return [];
}

export function OperationFields({ schema, values, onChange }: {
  schema: ParameterSchema; values: Parameters; onChange: (values: Parameters) => void;
}) {
  return (
    <div className="operation-fields">
      {[...fields(schema)].map(([name, field]) => (
        <ParameterField key={name} schema={field} label={labelFor(name, field)}
          required={schema.required?.includes(name) || false}
          value={Object.prototype.hasOwnProperty.call(values, name) ? values[name] : undefined}
          onChange={(value) => {
            const next = { ...values, [name]: value };
            if (value === undefined) delete next[name];
            onChange(next);
          }} />
      ))}
    </div>
  );
}

function ParameterField({ schema, label, required, value, onChange }: {
  schema: ParameterSchema; label: string; required: boolean; value: unknown;
  onChange: (value: unknown) => void;
}) {
  const type = fieldType(schema);
  const caption = required ? `${label} (obbligatorio)` : label;
  const help = schema.description ? <small className="sub">{schema.description}</small> : null;
  const choices = schema.const !== undefined ? [schema.const] : schema.enum;
  const nullable = Array.isArray(schema.type) && schema.type.includes("null");
  if (choices || type === "boolean" || type === "null") {
    const options = choices || (type === "null" ? [null] : nullable ? [true, false, null] : [true, false]);
    const selected = options.findIndex((option) => Object.is(option, value));
    const optionLabel = (option: unknown) => option === true ? "Sì" : option === false ? "No" : option === null ? "Nessun valore" : String(option);
    return (
      <label className="operation-input">{caption}
        <select value={selected < 0 ? "" : selected} onChange={(e) => onChange(e.target.value === "" ? undefined : options[Number(e.target.value)])}>
          <option value="">Seleziona…</option>
          {options.map((option, i) => <option key={i} value={i}>{optionLabel(option)}</option>)}
        </select>
        {help}
      </label>
    );
  }
  if (type === "object" || type === "array") {
    const items = Array.isArray(value) ? value : [];
    return (
      <fieldset className="operation-group">
        <legend>{caption}</legend>
        {help}
        {value === undefined && !required ? (
          <button type="button" className="ghost small" onClick={() => onChange(type === "object" ? initialParameters(schema) : [])}>Aggiungi {label}</button>
        ) : (
          <>
            {type === "object" ? <OperationFields schema={schema} values={(value as Parameters | undefined) ?? {}} onChange={onChange} /> : (
              <>
                {items.map((item, i) => (
                  <div className="operation-array-item" key={i}>
                    <ParameterField schema={schema.items || {}} label={`${label} ${i + 1}`} required value={item}
                      onChange={(next) => onChange(items.map((current, j) => i === j ? next : current))} />
                    <button type="button" className="ghost small" onClick={() => onChange(items.filter((_, j) => i !== j))}>Rimuovi {label} {i + 1}</button>
                  </div>
                ))}
                <button type="button" className="ghost small" disabled={items.length >= (schema.maxItems ?? Infinity)}
                  onClick={() => onChange([...items, undefined])}>Aggiungi voce</button>
              </>
            )}
            {!required ? <button type="button" className="ghost small" onClick={() => onChange(undefined)}>Rimuovi {label}</button> : null}
          </>
        )}
      </fieldset>
    );
  }
  const numeric = type === "number" || type === "integer";
  return (
    <label className="operation-input">{caption}
      {numeric ? (
        <input type="number" step={type === "integer" ? 1 : "any"} min={schema.minimum} max={schema.maximum}
          value={typeof value === "number" ? value : ""} onChange={(e) => onChange(e.target.value ? e.target.valueAsNumber : undefined)} />
      ) : (
        <textarea rows={schema.maxLength && schema.maxLength <= 180 ? 1 : 2}
          minLength={schema.minLength} maxLength={schema.maxLength} placeholder={schema.format === "date-time" ? "2026-11-20T15:00:00+01:00" : undefined}
          value={typeof value === "string" ? value : ""} onChange={(e) => onChange(e.target.value || undefined)} />
      )}
      {help}
    </label>
  );
}
