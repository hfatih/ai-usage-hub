import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type GoogleServiceStatus = "unknown" | "available" | "login_required" | "unavailable";
export interface GoogleAccountInput {
  id?: string;
  email: string;
  name: string;
  color: string;
  subscription: string;
  activationDate: string | null;
  expirationDate: string | null;
  enabled: boolean;
  archived: boolean;
  favorite: boolean;
  group: string;
  tags: string[];
  notes: string;
}
export interface GoogleAccount extends Omit<GoogleAccountInput, "id"> {
  id: string;
  antigravityId: string | null;
  lastUsed: number | null;
  createdAt: number;
  quotaRemaining: number | null;
  quotaUpdatedAt: number | null;
  quotaDetails: string | null;
}
export interface GoogleAssignment {
  accountId: string;
  role: string;
  services: string[];
}
export interface GoogleProjectInput {
  id?: string;
  name: string;
  description: string;
  notes: string;
  status: "active" | "paused" | "archived";
  assignments: GoogleAssignment[];
}
export interface GoogleProject extends Omit<GoogleProjectInput, "id"> {
  id: string;
  createdAt: number;
  updatedAt: number;
}
export interface GoogleUsageInput {
  accountId: string;
  serviceId: string;
  metric: string;
  value: number;
  maxValue: number | null;
  unit: string;
  recordedAt?: number;
}
export interface GoogleUsageRecord extends GoogleUsageInput {
  id: number;
  source: "manual" | "antigravity";
  recordedAt: number;
}
export interface GoogleService {
  id: string;
  name: string;
  url: string;
  favorite: boolean;
  lastAccountId: string | null;
  statuses: { accountId: string; status: GoogleServiceStatus; updatedAt: number }[];
}
export interface GoogleActivity {
  id: number;
  kind: string;
  accountId: string | null;
  projectId: string | null;
  title: string;
  detail: string;
  createdAt: number;
}
export interface GooglePreferences {
  notificationsEnabled: boolean;
  expirationWarningDays: number;
  autoBackup: boolean;
  backupRetention: number;
}
export interface GoogleBackup {
  name: string;
  path: string;
  createdAt: number;
  size: number;
}
export interface GoogleSnapshot {
  accounts: GoogleAccount[];
  projects: GoogleProject[];
  usage: GoogleUsageRecord[];
  services: GoogleService[];
  activity: GoogleActivity[];
  preferences: GooglePreferences;
  backups: GoogleBackup[];
}

export const googleApi = {
  snapshot: (): Promise<GoogleSnapshot> => invoke("google_control_snapshot"),
  saveAccount: (account: GoogleAccountInput): Promise<GoogleSnapshot> => invoke("google_save_account", { account }),
  removeAccount: (accountId: string): Promise<GoogleSnapshot> => invoke("google_remove_account", { accountId }),
  saveProject: (project: GoogleProjectInput): Promise<GoogleSnapshot> => invoke("google_save_project", { project }),
  deleteProject: (projectId: string): Promise<GoogleSnapshot> => invoke("google_delete_project", { projectId }),
  addUsage: (record: GoogleUsageInput): Promise<GoogleSnapshot> => invoke("google_add_usage", { record }),
  deleteUsage: (recordId: number): Promise<GoogleSnapshot> => invoke("google_delete_usage", { recordId }),
  launchService: (accountId: string, serviceId: string): Promise<GoogleSnapshot> => invoke("google_launch_service", { accountId, serviceId }),
  setServiceStatus: (accountId: string, serviceId: string, status: GoogleServiceStatus): Promise<GoogleSnapshot> =>
    invoke("google_set_service_status", { accountId, serviceId, status }),
  favoriteService: (serviceId: string, favorite: boolean): Promise<GoogleSnapshot> => invoke("google_favorite_service", { serviceId, favorite }),
  savePreferences: (preferences: GooglePreferences): Promise<GoogleSnapshot> => invoke("google_save_preferences", { preferences }),
  exportMetadata: (format: "json" | "csv"): Promise<string> => invoke("google_export_metadata", { format }),
  importMetadata: (data: string, format: "json" | "csv"): Promise<GoogleSnapshot> => invoke("google_import_metadata", { data, format }),
  backup: (): Promise<GoogleSnapshot> => invoke("google_create_backup"),
  restoreBackup: (path: string): Promise<GoogleSnapshot> => invoke("google_restore_backup", { path }),
  chooseBackup: async (): Promise<string | null> => {
    const selected = await open({ multiple: false, filters: [{ name: "AI Usage Hub backup", extensions: ["sqlite3", "db"] }] });
    return typeof selected === "string" ? selected : null;
  },
};
