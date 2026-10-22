import type { ConnectionConfig } from "@/types/database";
import { databaseManifestEntry } from "@/lib/database/databaseDriverManifest";

export function connectionServerVersion(config: ConnectionConfig): string | undefined {
  // The driver/Agent package version is not the database server version.
  return config.database_info?.productVersion?.trim() || undefined;
}

export function connectionServerVersionLabel(config: ConnectionConfig): string | undefined {
  const version = connectionServerVersion(config);
  if (!version) return undefined;
  // A protocol-compatible server may report the emulated product name.
  // Prefer the configured product so GaussDB/TiDB keep their own versions.
  const names = [config.driver_label, databaseManifestEntry(config.db_type)?.label, config.database_info?.productName];
  for (const name of names) {
    if (!name?.trim()) continue;
    const index = version.toLowerCase().indexOf(name.trim().toLowerCase());
    if (index < 0) continue;
    const suffix = version.slice(index + name.trim().length);
    const number = suffix.match(/\bv?(\d+(?:\.\d+)+(?:[-+][\w.-]+)?)(?:\b|$)/i);
    if (number) return `v${number[1]}`;
  }
  return `v${version.replace(/^v(?=\d)/i, "")}`;
}
