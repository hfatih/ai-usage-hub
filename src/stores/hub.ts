import { create } from "zustand";
import { api } from "../lib/ipc";
import type { AccountSummary, ActivityEvent, AntigravityAccount, AntigravityLiveSession, AppSettings, DashboardSnapshot, SecurityStatus, UsageHistoryPoint, UsageTrend } from "../types";

interface HubState {
  snapshot: DashboardSnapshot | null;
  settings: AppSettings | null;
  activity: ActivityEvent[];
  accounts: AccountSummary[];
  antigravityAccounts: AntigravityAccount[];
  liveSession: AntigravityLiveSession | null;
  usageHistory: UsageHistoryPoint[];
  usageTrends: UsageTrend[];
  notifications: ActivityEvent[];
  security: SecurityStatus | null;
  loading: boolean;
  refreshing: boolean;
  error: string | null;
  load: () => Promise<void>;
  refresh: () => Promise<void>;
  loadActivity: () => Promise<void>;
  loadAntigravityAccounts: () => Promise<void>;
  captureAntigravityAccount: () => Promise<AntigravityAccount>;
  addAntigravityAccount: (email: string, refreshToken: string) => Promise<AntigravityAccount>;
  deleteAntigravityAccount: (accountId: string) => Promise<void>;
  switchAntigravityAccount: (accountId: string) => Promise<AntigravityAccount>;
  toggleAntigravityAutoSwitch: (enabled: boolean) => Promise<void>;
  startAntigravityOAuthLogin: (openBrowser?: boolean) => Promise<AntigravityAccount>;
  completeAntigravityOAuthCode: (code: string) => Promise<AntigravityAccount>;
  exportAntigravityAccounts: () => Promise<string>;
  importAntigravityAccounts: (jsonData: string) => Promise<number>;
  refreshAllAntigravityAccounts: () => Promise<AntigravityAccount[]>;
  scanLocalAntigravityAccounts: () => Promise<AntigravityAccount[]>;
  syncAntigravitySession: () => Promise<AntigravityLiveSession>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  deleteAccount: (accountId: string) => Promise<void>;
  clearUsageHistory: () => Promise<void>;
  launchApp: (appId: string) => Promise<void>;
  setExecutable: (appId: string, path: string) => Promise<void>;
}

export const useHub = create<HubState>((set, get) => ({
  snapshot: null,
  settings: null,
  activity: [],
  accounts: [],
  antigravityAccounts: [],
  liveSession: null,
  usageHistory: [],
  usageTrends: [],
  notifications: [],
  security: null,
  loading: true,
  refreshing: false,
  error: null,
  load: async () => {
    // The backend asks for a reload every time something changes (at least once a minute). Only
    // the very first load shows the skeleton; later ones swap the data in place.
    set({ loading: get().snapshot === null, error: null });
    try {
      const [snapshot, fetchedSettings, accounts, antigravityAccounts, liveSession, usageHistory, notifications, security] = await Promise.all([
        api.dashboard(), api.settings(), api.accounts(), api.antigravityAccounts(), api.getAntigravityLiveSession(), api.usageHistory(), api.notifications(), api.security(),
      ]);
      // Keep the same object when nothing changed, so pages that edit a draft of the settings
      // (Settings, API Proxy) do not lose unsaved edits on every background reload.
      const current = get().settings;
      const settings = current && JSON.stringify(current) === JSON.stringify(fetchedSettings) ? current : fetchedSettings;
      set({ snapshot, settings, accounts, antigravityAccounts, liveSession, usageHistory, notifications, security, loading: false });
      // Secondary data: a failure here must not blank the dashboard.
      void Promise.all([api.activity(), api.usageTrends()])
        .then(([activity, usageTrends]) => set({ activity, usageTrends }))
        .catch(() => {});
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },
  refresh: async () => {
    set({ refreshing: true, error: null });
    try {
      const snapshot = await api.refresh();
      const [accounts, antigravityAccounts, liveSession, usageHistory, notifications, security] = await Promise.all([
        api.accounts(), api.antigravityAccounts(), api.getAntigravityLiveSession(), api.usageHistory(), api.notifications(), api.security(),
      ]);
      set({ snapshot, accounts, antigravityAccounts, liveSession, usageHistory, notifications, security, refreshing: false });
      void Promise.all([api.activity(), api.usageTrends()])
        .then(([activity, usageTrends]) => set({ activity, usageTrends }))
        .catch(() => {});
    } catch (error) {
      set({ error: String(error), refreshing: false });
    }
  },
  loadActivity: async () => set({ activity: await api.activity() }),
  loadAntigravityAccounts: async () => set({ antigravityAccounts: await api.antigravityAccounts() }),
  captureAntigravityAccount: async () => {
    const acc = await api.captureAntigravityAccount();
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    return acc;
  },
  addAntigravityAccount: async (email, refreshToken) => {
    const acc = await api.addAntigravityAccount(email, refreshToken);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    return acc;
  },
  deleteAntigravityAccount: async (accountId) => {
    await api.deleteAntigravityAccount(accountId);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
  },
  switchAntigravityAccount: async (accountId) => {
    const switched = await api.switchAntigravityAccount(accountId);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    // Trigger dashboard refresh so live usage updates immediately
    get().refresh().catch(() => {});
    return switched;
  },
  toggleAntigravityAutoSwitch: async (enabled) => {
    await api.toggleAntigravityAutoSwitch(enabled);
    const settings = await api.settings();
    set({ settings });
  },
  startAntigravityOAuthLogin: async (openBrowser) => {
    const acc = await api.startAntigravityOAuthLogin(openBrowser);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    get().refresh().catch(() => {});
    return acc;
  },
  completeAntigravityOAuthCode: async (code) => {
    const acc = await api.completeAntigravityOAuthCode(code);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    get().refresh().catch(() => {});
    return acc;
  },
  exportAntigravityAccounts: async () => {
    return await api.exportAntigravityAccounts();
  },
  importAntigravityAccounts: async (jsonData) => {
    const count = await api.importAntigravityAccounts(jsonData);
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    get().refresh().catch(() => {});
    return count;
  },
  refreshAllAntigravityAccounts: async () => {
    const antigravityAccounts = await api.refreshAllAntigravityAccounts();
    set({ antigravityAccounts });
    get().refresh().catch(() => {});
    return antigravityAccounts;
  },
  scanLocalAntigravityAccounts: async () => {
    const accounts = await api.scanLocalAntigravityAccounts();
    const antigravityAccounts = await api.antigravityAccounts();
    set({ antigravityAccounts });
    get().refresh().catch(() => {});
    return accounts;
  },
  syncAntigravitySession: async () => {
    const liveSession = await api.syncAntigravitySession();
    const [antigravityAccounts, accounts] = await Promise.all([
      api.antigravityAccounts(),
      api.accounts(),
    ]);
    set({ liveSession, antigravityAccounts, accounts });
    get().refresh().catch(() => {});
    return liveSession;
  },
  // Failures show in the error banner (callers often fire and forget) and are still thrown for
  // callers that show them next to the field.
  saveSettings: async (settings) => {
    try {
      set({ settings: await api.saveSettings(settings), error: null });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },
  launchApp: async (appId) => {
    try {
      await api.launchApp(appId);
    } catch (error) {
      set({ error: String(error) });
    }
  },
  setExecutable: async (appId, path) => {
    try {
      await api.setExecutable(appId, path);
      await get().refresh();
    } catch (error) {
      set({ error: String(error) });
    }
  },
  deleteAccount: async (accountId) => {
    await api.deleteAccount(accountId);
    const [accounts, usageHistory, security] = await Promise.all([api.accounts(), api.usageHistory(), api.security()]);
    set({ accounts, usageHistory, security });
  },
  clearUsageHistory: async () => {
    await api.clearUsageHistory();
    const [accounts, usageHistory, notifications, security] = await Promise.all([
      api.accounts(), api.usageHistory(), api.notifications(), api.security(),
    ]);
    set({ accounts, usageHistory, notifications, security });
  },
}));
