// Tiny formatting helpers used across the admin panel.

export function formatTimestamp(ts: number | null | undefined): string {
  if (!ts && ts !== 0) return "—";
  // Backend timestamps are unix seconds.
  const ms = ts < 1e12 ? ts * 1000 : ts;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "—";
  const pad = (n: number) => n.toString().padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

export function formatLatency(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function maskKey(key: string): string {
  if (!key) return "";
  if (key.length <= 8) return "****";
  return `${key.slice(0, 3)}****${key.slice(-4)}`;
}

export function truncate(s: string | null | undefined, max: number): string {
  if (!s) return "";
  if (s.length <= max) return s;
  return `${s.slice(0, max)}…`;
}

export function formatNumber(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return "—";
  return new Intl.NumberFormat("zh-CN").format(n);
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let val = bytes / 1024;
  let i = 0;
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024;
    i += 1;
  }
  return `${val.toFixed(val >= 100 ? 0 : val >= 10 ? 1 : 2)} ${units[i]}`;
}

export function formatDurationSince(sinceTs: number): string {
  if (!sinceTs) return "—";
  const ms = (sinceTs < 1e12 ? sinceTs * 1000 : sinceTs) - Date.now();
  const secs = Math.max(0, Math.round(-ms / 1000));
  if (secs < 60) return `${secs} 秒`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} 分 ${secs % 60} 秒`;
  const hours = Math.floor(mins / 60);
  return `${hours} 时 ${mins % 60} 分`;
}

export function tryPrettyJson(text: string | null | undefined): string {
  if (!text) return "";
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}