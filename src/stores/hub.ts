import { create } from "zustand";
import { api } from "../lib/ipc";
import type { AccountSummary, ActivityEvent, AntigravityAccount, AntigravityLiveSession, AppSettings, DashboardSnapshot, SecurityStatus, UsageHistoryPoint } from "../types";

interface HubState {
  snapshot: DashboardSnapshot | null;
  settings: AppSettings | null;
  activity: ActivityEvent[];
  accounts: AccountSummary[];
  antigravityAccounts: AntigravityAccount[];
  liveSession: AntigravityLiveSession | null;
  usageHistory: UsageHistoryPoint[];
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
  startAntigravityOAuthLogin: () => Promise<AntigravityAccount>;
  completeAntigravityOAuthCode: (code: string) => Promise<AntigravityAccount>;
  exportAntigravityAccounts: () => Promise<string>;
  importAntigravityAccounts: (jsonData: string) => Promise<number>;
  refreshAllAntigravityAccounts: () => Promise<AntigravityAccount[]>;
  scanLocalAntigravityAccounts: () => Promise<AntigravityAccount[]>;
  syncAntigravitySession: () => Promise<AntigravityLiveSession>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  deleteAccount: (accountId: string) => Promise<void>;
  clearUsageHistory: () => Promise<void>;
}

export const useHub = create<HubState>((set, get) => ({
  snapshot: null,
  settings: null,
  activity: [],
  accounts: [],
  antigravityAccounts: [],
  liveSession: null,
  usageHistory: [],
  notifications: [],
  security: null,
  loading: true,
  refreshing: false,
  error: null,
  load: async () => {
    set({ loading: true, error: null });
    try {
      const [snapshot, settings, accounts, antigravityAccounts, liveSession, usageHistory, notifications, security] = await Promise.all([
        api.dashboard(), api.settings(), api.accounts(), api.antigravityAccounts(), api.getAntigravityLiveSession(), api.usageHistory(), api.notifications(), api.security(),
      ]);
      set({ snapshot, settings, accounts, antigravityAccounts, liveSession, usageHistory, notifications, security, loading: false });
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
  startAntigravityOAuthLogin: async () => {
    const acc = await api.startAntigravityOAuthLogin();
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
  saveSettings: async (settings) => set({ settings: await api.saveSettings(settings) }),
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
