import { Archive } from "lucide-react";
import {
  googleApi,
  type GoogleAccount,
  type GoogleProject,
  type GoogleSnapshot,
} from "../../lib/googleControl";
import { AccountForm } from "./AccountForm";
import { ProjectForm } from "./Projects";
import { Modal, useGoogleText, type Run } from "./shared";

export type Dialog =
  | { kind: "account"; account: GoogleAccount | null }
  | { kind: "project"; project: GoogleProject | null }
  | {
      kind: "confirm";
      title: string;
      message: string;
      detail?: string;
      label: string;
      action: () => Promise<GoogleSnapshot>;
    };

export function ControlDialog({
  dialog,
  snapshot,
  busy,
  error,
  run,
  onClose,
}: {
  dialog: Dialog | null;
  snapshot: GoogleSnapshot | null;
  busy: boolean;
  error: string | null;
  run: Run;
  onClose: () => void;
}) {
  const text = useGoogleText();
  return (
    <>
      {dialog && (
        <Modal
          title={
            dialog.kind === "account"
              ? dialog.account
                ? text("Edit Google account", "Google hesabını düzenle")
                : text("Add Google account", "Google hesabı ekle")
              : dialog.kind === "project"
                ? dialog.project
                  ? text("Edit project", "Projeyi düzenle")
                  : text("New project", "Yeni proje")
                : dialog.title
          }
          onClose={() => onClose()}
          busy={busy}
          error={error}
        >
          {dialog.kind === "account" && (
            <AccountForm
              account={dialog.account}
              busy={busy}
              onCancel={() => onClose()}
              onSave={async (account) => {
                if (
                  await run(
                    () => googleApi.saveAccount(account),
                    text("Account saved.", "Hesap kaydedildi."),
                  )
                )
                  onClose();
              }}
            />
          )}
          {dialog.kind === "project" && snapshot && (
            <ProjectForm
              project={dialog.project}
              snapshot={snapshot}
              busy={busy}
              onCancel={() => onClose()}
              onSave={async (project) => {
                if (
                  await run(
                    () => googleApi.saveProject(project),
                    text("Project saved.", "Proje kaydedildi."),
                  )
                )
                  onClose();
              }}
            />
          )}
          {dialog.kind === "confirm" && (
            <div className="gc-confirm">
              <Archive size={27} />
              <p>{dialog.message}</p>
              {dialog.detail && <code>{dialog.detail}</code>}
              <div className="gc-form-actions">
                <button
                  className="secondary-button"
                  disabled={busy}
                  onClick={() => onClose()}
                >
                  {text("Cancel", "Vazgeç")}
                </button>
                <button
                  className="primary-button gc-destructive"
                  disabled={busy}
                  onClick={() =>
                    void run(
                      dialog.action,
                      text("Action completed.", "İşlem tamamlandı."),
                    ).then((ok) => {
                      if (ok) {
                        onClose();
                        if (dialog.detail)
                          window.dispatchEvent(
                            new Event("google-control-restored"),
                          );
                      }
                    })
                  }
                >
                  {busy ? text("Working…", "İşleniyor…") : dialog.label}
                </button>
              </div>
            </div>
          )}
        </Modal>
      )}
    </>
  );
}
