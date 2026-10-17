import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { AccountSummary, ActivityEvent, AntigravityAccount, AntigravityLiveSession, AppSettings, CliConfigStatus, DashboardSnapshot, ProxyModel, ProxyStatus, SecurityStatus, UsageHistoryPoint } from "../types";
import { developmentAccounts, developmentAntigravityAccounts, developmentSecurity, developmentSettings, developmentSnapshot, developmentUsageHistory } from "./mock";
import { isMac } from "./platform";

const inTauri = () => "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) throw new Error("Desktop backend is not available in browser preview.");
  return invoke<T>(command, args);
}

export const api = {
  dashboard: (): Promise<DashboardSnapshot> =>
    inTauri() ? call("get_dashboard") : Promise.resolve(structuredClone(developmentSnapshot)),
  refresh: (): Promise<DashboardSnapshot> =>
    inTauri() ? call("refresh_processes") : Promise.resolve(structuredClone(developmentSnapshot)),
  activity: (): Promise<ActivityEvent[]> =>
    inTauri() ? call("get_activity") : Promise.resolve([]),
  accounts: (): Promise<AccountSummary[]> =>
    inTauri() ? call("get_accounts") : Promise.resolve(structuredClone(developmentAccounts)),
  deleteAccount: (accountId: string): Promise<boolean> =>
    inTauri() ? call("delete_account", { accountId }) : Promise.resolve(true),
  usageHistory: (): Promise<UsageHistoryPoint[]> =>
    inTauri() ? call("get_usage_history") : Promise.resolve(structuredClone(developmentUsageHistory)),
  notifications: (): Promise<ActivityEvent[]> =>
    inTauri() ? call("get_notifications") : Promise.resolve([]),
  security: (): Promise<SecurityStatus> =>
    inTauri() ? call("get_security_status") : Promise.resolve({ ...developmentSecurity }),
  clearUsageHistory: (): Promise<number> =>
    inTauri() ? call("clear_usage_history") : Promise.resolve(0),
  settings: (): Promise<AppSettings> =>
    inTauri() ? call("get_settings") : Promise.resolve({ ...developmentSettings }),
  saveSettings: (settings: AppSettings): Promise<AppSettings> =>
    inTauri() ? call("update_settings", { settings }) : Promise.resolve(settings),
  launchApp: (appId: string): Promise<void> => call("launch_app", { appId }),
  setMonitoringPaused: (paused: boolean): Promise<boolean> =>
    inTauri() ? call("set_monitoring_paused", { paused }) : Promise.resolve(paused),
  chooseExecutable: async (filterName: string): Promise<string | null> => {
    if (!inTauri()) return null;
    const result = await open(isMac
      ? { multiple: false, defaultPath: "/Applications", filters: [{ name: filterName, extensions: ["app"] }] }
      : { multiple: false, filters: [{ name: filterName, extensions: ["exe"] }] });
    return typeof result === "string" ? result : null;
  },
  setExecutable: (appId: string, executablePath: string): Promise<void> =>
    call("set_app_executable", { appId, executablePath }),
  antigravityAccounts: (): Promise<AntigravityAccount[]> =>
    inTauri() ? call("get_antigravity_accounts") : Promise.resolve(structuredClone(developmentAntigravityAccounts)),
  captureAntigravityAccount: (): Promise<AntigravityAccount> =>
    inTauri() ? call("capture_current_antigravity_account") : Promise.resolve(developmentAntigravityAccounts[0]),
  addAntigravityAccount: (email: string, refreshToken: string): Promise<AntigravityAccount> =>
    inTauri() ? call("add_antigravity_account", { email, refreshToken }) : Promise.resolve({
      id: `antigravity:${email}`,
      email,
      displayName: null,
      accessToken: "mock",
      refreshToken,
      idToken: null,
      projectId: null,
      expiryTimestamp: Math.floor(Date.now() / 1000) + 3600,
      status: "active",
      isCurrent: false,
      lastUsed: Math.floor(Date.now() / 1000),
      quotaRemainingPercent: 100,
    }),
  deleteAntigravityAccount: (accountId: string): Promise<boolean> =>
    inTauri() ? call("delete_antigravity_account", { accountId }) : Promise.resolve(true),
  switchAntigravityAccount: (accountId: string): Promise<AntigravityAccount> =>
    inTauri() ? call("switch_antigravity_account", { accountId }) : Promise.resolve(developmentAntigravityAccounts[0]),
  toggleAntigravityAutoSwitch: (enabled: boolean): Promise<boolean> =>
    inTauri() ? call("toggle_antigravity_auto_switch", { enabled }) : Promise.resolve(enabled),
  getAntigravityOAuthUrl: (): Promise<string> =>
    inTauri() ? call("get_antigravity_oauth_url") : Promise.resolve("https://accounts.google.com/mock"),
  startAntigravityOAuthLogin: (openBrowser = true): Promise<AntigravityAccount> =>
    inTauri() ? call("start_antigravity_oauth_login", { openBrowser }) :Promise.resolve(developmentAntigravityAccounts[0]),
  completeAntigravityOAuthCode: (code: string): Promise<AntigravityAccount> =>
    inTauri() ? call("complete_antigravity_oauth_code", { code }) : Promise.resolve(developmentAntigravityAccounts[0]),
  exportAntigravityAccounts: (): Promise<string> =>
    inTauri() ? call("export_antigravity_accounts") : Promise.resolve("[]"),
  importAntigravityAccounts: (jsonData: string): Promise<number> =>
    inTauri() ? call("import_antigravity_accounts", { jsonData }) : Promise.resolve(0),
  refreshAllAntigravityAccounts: (): Promise<AntigravityAccount[]> =>
    inTauri() ? call("refresh_all_antigravity_accounts") : Promise.resolve(developmentAntigravityAccounts),
  scanLocalAntigravityAccounts: (): Promise<AntigravityAccount[]> =>
    inTauri() ? call("scan_local_antigravity_accounts") : Promise.resolve(developmentAntigravityAccounts),
  getAntigravityLiveSession: (): Promise<AntigravityLiveSession> =>
    inTauri()
      ? call("get_antigravity_live_session")
      : Promise.resolve({
          isRunning: true,
          email: "primary.dev@example.com",
          name: "Fatih",
          plan: "Google AI Pro",
          source: "LanguageServer (Mock)",
          port: 63251,
          verifiedAt: Math.floor(Date.now() / 1000),
          quotaDetails: null,
          quotaRemainingPercent: 82,
        }),
  syncAntigravitySession: (): Promise<AntigravityLiveSession> =>
    inTauri()
      ? call("sync_antigravity_session")
      : Promise.resolve({
          isRunning: true,
          email: "primary.dev@example.com",
          name: "Fatih",
          plan: "Google AI Pro",
          source: "LanguageServer (Mock)",
          port: 63251,
          verifiedAt: Math.floor(Date.now() / 1000),
          quotaDetails: null,
          quotaRemainingPercent: 82,
        }),
  proxyStatus: (): Promise<ProxyStatus> =>
    inTauri()
      ? call("get_proxy_status")
      : Promise.resolve({
          running: true,
          port: 8046,
          totalRequests: 0,
          successRequests: 0,
          failedRequests: 0,
          lastLatencyMs: 0,
          activeAccountEmail: "mock@example.com",
        }),
  regenerateProxyApiKey: (): Promise<AppSettings> =>
    inTauri() ? call("regenerate_proxy_api_key") : Promise.resolve({ ...developmentSettings, proxyApiKey: "sk-hub-preview-" + Date.now().toString(16) }),
  proxyModels: (): Promise<ProxyModel[]> =>
    inTauri() ? call("get_proxy_models") : Promise.resolve([]),
  testProxyPrompt: (prompt: string, model?: string): Promise<{ text: string; latencyMs: number }> =>
    inTauri()
      ? call("test_proxy_prompt", { prompt, model })
      : Promise.resolve({ text: "Simulated response from mock proxy.", latencyMs: 280 }),
  clearProxyLogs: (): Promise<boolean> =>
    inTauri() ? call("clear_proxy_logs") : Promise.resolve(true),
  setAntigravityAccountProxy: (accountId: string, proxyUrl: string): Promise<string | null> =>
    inTauri() ? call("set_antigravity_account_proxy", { accountId, proxyUrl }) : Promise.resolve(proxyUrl || null),
  closeAntigravity: (force: boolean): Promise<boolean> =>
    inTauri() ? call("close_antigravity", { force }) : Promise.resolve(true),
  acknowledgeProxyRisk: (): Promise<AppSettings> =>
    inTauri() ? call("acknowledge_proxy_risk") : Promise.resolve({ ...developmentSettings, proxyRiskAcknowledged: true }),
  setProviderKey: (providerId: string, key: string): Promise<DashboardSnapshot> =>
    inTauri() ? call("set_provider_key", { providerId, key }) : Promise.resolve(structuredClone(developmentSnapshot)),
  cliConfigStatus: (): Promise<CliConfigStatus> =>
    inTauri() ? call("get_cli_config_status") : Promise.resolve({ claude: false, opencode: false }),
  applyCliConfig: (tool: string): Promise<CliConfigStatus> =>
    inTauri() ? call("apply_cli_config", { tool }) : Promise.resolve({ claude: tool === "claude", opencode: tool === "opencode" }),
  removeCliConfig: (tool: string): Promise<CliConfigStatus> =>
    inTauri() ? call("remove_cli_config", { tool }) : Promise.resolve({ claude: false, opencode: false }),
};
