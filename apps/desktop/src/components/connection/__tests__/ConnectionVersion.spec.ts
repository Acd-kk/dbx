// @vitest-environment happy-dom
import { createApp, h, nextTick, reactive, type App } from "vue";
import { createI18n } from "vue-i18n";
import { afterEach, describe, expect, it, vi } from "vitest";
import ConnectionVersion from "@/components/connection/ConnectionVersion.vue";
import en from "@/i18n/locales/en";
import zhCN from "@/i18n/locales/zh-CN";
import type { ConnectionConfig, DatabaseType } from "@/types/database";

const state = reactive({ config: undefined as ConnectionConfig | undefined });
vi.mock("@/stores/connectionStore", () => ({ useConnectionStore: () => ({ getConfig: () => state.config }) }));
const apps: App[] = [];

afterEach(() => {
  for (const app of apps.splice(0)) app.unmount();
  document.body.replaceChildren();
  state.config = undefined;
});

async function mount(dbType: DatabaseType, productVersion?: string, locale = "en") {
  state.config = { id: "example", name: "Example", db_type: dbType, host: "localhost", port: 0, username: "", password: "", database_info: { productVersion } };
  const container = document.createElement("div");
  document.body.append(container);
  const app = createApp({ render: () => h(ConnectionVersion, { connectionId: "example" }) });
  app.use(createI18n({ legacy: false, locale, messages: { en, "zh-CN": zhCN } }));
  apps.push(app);
  app.mount(container);
  await nextTick();
  return container;
}

describe("connection server version", () => {
  it.each([
    ["mysql", "8.0", "v8.0"],
    ["redis", "7.1.1", "v7.1.1"],
    ["qdrant", "v1.12.0", "v1.12.0"],
    ["elasticsearch", "8.16.0", "v8.16.0"],
  ] as const)("displays only the %s server version with a single v prefix", async (dbType, version, expected) => {
    const container = await mount(dbType, version);
    expect(container.textContent?.trim()).toBe(expected);
    expect(container.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe(`Server version: ${version}`);
  });

  it("shows an unknown version until background metadata arrives", async () => {
    const container = await mount("milvus");
    state.config!.database_info = { driverVersion: "0.1.99" };
    await nextTick();
    expect(container.textContent?.trim()).toBe("Version unknown");
    state.config!.database_info = { productVersion: "2.5.1" };
    await nextTick();
    expect(container.textContent?.trim()).toBe("v2.5.1");
  });

  it("localizes the missing version without adding a v prefix", async () => {
    const container = await mount("redis", undefined, "zh-CN");
    expect(container.textContent?.trim()).toBe("版本未知");
  });

  it("preserves the complete version in the title and reacts to connection edits", async () => {
    const version = "PostgreSQL 16.4 on x86_64-pc-linux-gnu, compiled by gcc, 64-bit";
    const container = await mount("postgres", version);
    expect(container.textContent?.trim()).toBe("v16.4");
    expect(container.querySelector("[title]")?.getAttribute("title")).toContain(version);
    state.config = { ...state.config!, db_type: "redis", database_info: { productVersion: "7.1.1" } };
    await nextTick();
    expect(container.textContent?.trim()).toBe("v7.1.1");
    state.config = undefined;
    await nextTick();
    expect(container.textContent).toBe("");
  });
});
