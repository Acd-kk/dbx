import { describe, expect, it } from "vitest";
import { connectionServerVersion, connectionServerVersionLabel } from "@/lib/connection/connectionServerVersion";
import type { ConnectionConfig } from "@/types/database";

function connection(productVersion?: string, overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return { id: "example", name: "Example", db_type: "mysql", host: "localhost", port: 0, username: "", password: "", database_info: { productVersion }, ...overrides };
}

describe("connection server version labels", () => {
  it.each([
    ["8.0", "v8.0"],
    ["7.1.1", "v7.1.1"],
    [" v8.0 ", "v8.0"],
    ["V7.1.1", "v7.1.1"],
    ["1.12.0-rc.1", "v1.12.0-rc.1"],
  ])("formats %s with a single v prefix", (version, expected) => {
    expect(connectionServerVersionLabel(connection(version))).toBe(expected);
  });

  it("shortens a PostgreSQL banner while preserving the complete metadata", () => {
    const config = connection("  PostgreSQL 16.4 on x86_64  ", { db_type: "postgres" });
    expect(connectionServerVersion(config)).toBe("PostgreSQL 16.4 on x86_64");
    expect(connectionServerVersionLabel(config)).toBe("v16.4");
  });

  it("selects the product version instead of a PostgreSQL compatibility version", () => {
    const config = connection(undefined, { db_type: "gaussdb", driver_label: "GaussDB", database_info: { productName: "PostgreSQL", productVersion: "PostgreSQL 9.2.4 (GaussDB 9.1.1)" } });
    expect(connectionServerVersionLabel(config)).toBe("v9.1.1");
  });

  it("selects the TiDB version after the MySQL protocol compatibility prefix", () => {
    const config = connection("5.7.25-TiDB-v7.1.1", { driver_label: "TiDB" });
    expect(connectionServerVersionLabel(config)).toBe("v7.1.1");
  });

  it.each([undefined, "", "   "])("does not use a driver version when the server version is %s", (version) => {
    const config = connection(version);
    config.database_info!.driverVersion = "0.1.99";
    expect(connectionServerVersion(config)).toBeUndefined();
    expect(connectionServerVersionLabel(config)).toBeUndefined();
  });
});
