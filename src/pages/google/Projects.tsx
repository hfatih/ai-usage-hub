import { useState } from "react";
import { Edit3, FolderOpen, Plus, Trash2 } from "lucide-react";
import type {
  GoogleProject,
  GoogleProjectInput,
  GoogleSnapshot,
} from "../../lib/googleControl";
import {
  AccountSelect,
  projectInput,
  accountName,
  Empty,
  Field,
  timestamp,
  useGoogleText,
} from "./shared";

export function ProjectForm({
  project,
  snapshot,
  busy,
  onSave,
  onCancel,
}: {
  project: GoogleProject | null;
  snapshot: GoogleSnapshot;
  busy: boolean;
  onSave: (p: GoogleProjectInput) => Promise<void>;
  onCancel: () => void;
}) {
  const text = useGoogleText();
  const [form, setForm] = useState<GoogleProjectInput>(
    project
      ? projectInput(project)
      : {
          name: "",
          description: "",
          notes: "",
          status: "active",
          assignments: [],
        },
  );
  const accounts = snapshot.accounts.filter(
    (a) =>
      !a.archived ||
      form.assignments.some((assignment) => assignment.accountId === a.id),
  );
  const updateAssignment = (
    index: number,
    change: Partial<GoogleProjectInput["assignments"][number]>,
  ) =>
    setForm((current) => ({
      ...current,
      assignments: current.assignments.map((a, i) =>
        i === index ? { ...a, ...change } : a,
      ),
    }));
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void onSave({ ...form, name: form.name.trim() });
      }}
    >
      <div className="gc-form-grid">
        <Field label={text("Project name", "Proje adı")}>
          <input
            required
            maxLength={160}
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
        </Field>
        <Field label={text("Status", "Durum")}>
          <select
            value={form.status}
            onChange={(e) =>
              setForm({
                ...form,
                status: e.target.value as GoogleProjectInput["status"],
              })
            }
          >
            <option value="active">{text("Active", "Etkin")}</option>
            <option value="paused">{text("Paused", "Duraklatıldı")}</option>
            <option value="archived">{text("Archived", "Arşiv")}</option>
          </select>
        </Field>
        <Field label={text("Description", "Açıklama")} wide>
          <textarea
            value={form.description}
            rows={2}
            maxLength={10000}
            onChange={(e) => setForm({ ...form, description: e.target.value })}
          />
        </Field>
        <Field label={text("Notes", "Notlar")} wide>
          <textarea
            value={form.notes}
            rows={2}
            maxLength={10000}
            onChange={(e) => setForm({ ...form, notes: e.target.value })}
          />
        </Field>
      </div>
      <div className="gc-assignment-heading">
        <h3>
          {text("Account roles and services", "Hesap rolleri ve hizmetler")}
        </h3>
        <button
          type="button"
          className="text-button"
          disabled={busy || form.assignments.length >= accounts.length}
          onClick={() =>
            setForm({
              ...form,
              assignments: [
                ...form.assignments,
                { accountId: "", role: "", services: [] },
              ],
            })
          }
        >
          <Plus size={14} />
          {text("Assign account", "Hesap ata")}
        </button>
      </div>
      {form.assignments.map((assignment, index) => (
        <fieldset key={index} className="gc-assignment">
          <legend>
            {text(`Assignment ${index + 1}`, `Atama ${index + 1}`)}
          </legend>
          <div className="gc-assignment-row">
            <AccountSelect
              accounts={accounts.filter(
                (a) =>
                  a.id === assignment.accountId ||
                  !form.assignments.some((x) => x.accountId === a.id),
              )}
              value={assignment.accountId}
              onChange={(id) => updateAssignment(index, { accountId: id })}
              label={text("Assigned account", "Atanan hesap")}
              disabled={busy}
            />
            <button
              type="button"
              className="icon-button"
              aria-label={text("Remove assignment", "Atamayı kaldır")}
              disabled={busy}
              onClick={() =>
                setForm({
                  ...form,
                  assignments: form.assignments.filter((_, i) => i !== index),
                })
              }
            >
              <Trash2 size={15} />
            </button>
          </div>
          <Field label={text("Role / purpose", "Rol / amaç")}>
            <input
              value={assignment.role}
              maxLength={500}
              onChange={(e) =>
                updateAssignment(index, { role: e.target.value })
              }
            />
          </Field>
          <div className="gc-service-checkboxes">
            {snapshot.services.map((service) => (
              <label key={service.id}>
                <input
                  type="checkbox"
                  checked={assignment.services.includes(service.id)}
                  onChange={(e) =>
                    updateAssignment(index, {
                      services: e.target.checked
                        ? [...assignment.services, service.id]
                        : assignment.services.filter((id) => id !== service.id),
                    })
                  }
                />
                {service.name}
              </label>
            ))}
          </div>
        </fieldset>
      ))}
      {!form.assignments.length && (
        <p className="gc-hint">
          {text(
            "Add accounts to describe each account's role in this project.",
            "Her hesabın bu projedeki rolünü tanımlamak için hesap ata.",
          )}
        </p>
      )}
      <p className="gc-hint">
        {text(
          "Projects organize your work. Assignments do not transfer files, credits, notebooks, or subscription benefits.",
          "Projeler çalışmalarını düzenler. Atamalar dosya, kredi, not defteri veya abonelik hakkı taşımaz.",
        )}
      </p>
      <footer className="gc-form-actions">
        <button
          type="button"
          className="secondary-button"
          disabled={busy}
          onClick={onCancel}
        >
          {text("Cancel", "Vazgeç")}
        </button>
        <button className="primary-button" disabled={busy}>
          {busy
            ? text("Saving…", "Kaydediliyor…")
            : text("Save project", "Projeyi kaydet")}
        </button>
      </footer>
    </form>
  );
}
export function Projects({
  snapshot,
  busy,
  onAdd,
  onEdit,
  onDelete,
}: {
  snapshot: GoogleSnapshot;
  busy: boolean;
  onAdd: () => void;
  onEdit: (p: GoogleProject) => void;
  onDelete: (p: GoogleProject) => void;
}) {
  const text = useGoogleText();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const projects = snapshot.projects.filter(
    (p) =>
      (status === "all" || p.status === status) &&
      [p.name, p.description, p.notes]
        .join(" ")
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  return (
    <>
      <div className="gc-toolbar">
        <input
          className="gc-project-search"
          aria-label={text("Search projects", "Projelerde ara")}
          value={query}
          placeholder={text("Search projects…", "Proje ara…")}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select
          aria-label={text("Project status filter", "Proje durum filtresi")}
          value={status}
          onChange={(e) => setStatus(e.target.value)}
        >
          <option value="all">{text("All statuses", "Tüm durumlar")}</option>
          <option value="active">{text("Active", "Etkin")}</option>
          <option value="paused">{text("Paused", "Duraklatıldı")}</option>
          <option value="archived">{text("Archived", "Arşiv")}</option>
        </select>
        <button className="primary-button" disabled={busy} onClick={onAdd}>
          <Plus size={15} />
          {text("New project", "Yeni proje")}
        </button>
      </div>
      {!projects.length ? (
        <Empty>
          <FolderOpen size={24} />
          <p>
            {text(
              "No projects match this view. Create a project and assign your accounts.",
              "Bu görünüme uyan proje yok. Proje oluşturup hesaplarını ata.",
            )}
          </p>
          <button className="secondary-button" onClick={onAdd}>
            {text("Create project", "Proje oluştur")}
          </button>
        </Empty>
      ) : (
        <div className="gc-project-grid">
          {projects.map((project) => (
            <article className="glass-panel gc-project-card" key={project.id}>
              <div className="gc-card-heading">
                <FolderOpen size={21} />
                <div>
                  <h3>{project.name}</h3>
                  <small>
                    {text("Created", "Oluşturuldu")}:{" "}
                    {timestamp(project.createdAt, text)}
                  </small>
                </div>
                <span className="gc-badge">
                  {project.status === "active"
                    ? text("Active", "Etkin")
                    : project.status === "paused"
                      ? text("Paused", "Duraklatıldı")
                      : text("Archived", "Arşiv")}
                </span>
              </div>
              {project.description && <p>{project.description}</p>}
              <div className="gc-project-assignments">
                {project.assignments.map((assignment) => (
                  <div key={assignment.accountId}>
                    <strong>
                      {accountName(
                        snapshot.accounts.find(
                          (a) => a.id === assignment.accountId,
                        ),
                      )}
                    </strong>
                    <span>
                      {assignment.role ||
                        text("No role set", "Rol belirtilmedi")}
                    </span>
                    <small>
                      {assignment.services
                        .map(
                          (id) =>
                            snapshot.services.find((s) => s.id === id)?.name ??
                            id,
                        )
                        .join(" · ") ||
                        text("No services assigned", "Hizmet atanmadı")}
                    </small>
                  </div>
                ))}
                {!project.assignments.length && (
                  <span className="gc-muted">
                    {text("No accounts assigned", "Hesap atanmadı")}
                  </span>
                )}
              </div>
              {project.notes && (
                <details>
                  <summary>{text("Notes", "Notlar")}</summary>
                  <p className="gc-preserve">{project.notes}</p>
                </details>
              )}
              <details>
                <summary>{text("Project activity", "Proje etkinliği")}</summary>
                <div className="gc-project-history">
                  {snapshot.activity
                    .filter((a) => a.projectId === project.id)
                    .slice(0, 15)
                    .map((a) => (
                      <p key={a.id}>
                        <strong>{a.title}</strong>
                        <small>
                          {timestamp(a.createdAt, text)} · {a.detail}
                        </small>
                      </p>
                    ))}
                  {!snapshot.activity.some(
                    (a) => a.projectId === project.id,
                  ) && (
                    <p>
                      {text(
                        "No activity recorded yet.",
                        "Henüz etkinlik kaydı yok.",
                      )}
                    </p>
                  )}
                </div>
              </details>
              <div className="gc-card-footer">
                <small>
                  {text("Updated", "Güncellendi")}:{" "}
                  {timestamp(project.updatedAt, text)}
                </small>
                <div className="gc-inline-actions">
                  <button
                    className="icon-button"
                    disabled={busy}
                    onClick={() => onEdit(project)}
                    aria-label={text("Edit project", "Projeyi düzenle")}
                  >
                    <Edit3 size={15} />
                  </button>
                  <button
                    className="icon-button"
                    disabled={busy}
                    onClick={() => onDelete(project)}
                    aria-label={text("Delete project", "Projeyi sil")}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>
            </article>
          ))}
        </div>
      )}
    </>
  );
}
