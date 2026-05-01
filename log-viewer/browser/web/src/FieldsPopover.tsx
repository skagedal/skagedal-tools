import { useEffect, useRef, useState } from "react";
import type { ManagedField } from "./types";

interface Props {
  fields: ManagedField[];
  anchorRef: React.RefObject<HTMLElement>;
  onClose: () => void;
  onToggle: (index: number) => void;
  onReorder: (from: number, to: number) => void;
}

export function FieldsPopover({ fields, anchorRef, onClose, onToggle, onReorder }: Props) {
  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onClick = (ev: MouseEvent) => {
      const t = ev.target as Node | null;
      if (!t) return;
      if (popoverRef.current?.contains(t)) return;
      if (anchorRef.current?.contains(t)) return;
      onClose();
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [onClose, anchorRef]);

  return (
    <div
      ref={popoverRef}
      className="popover"
      role="dialog"
      aria-label="Manage fields"
    >
      <div className="popover-head">
        <strong>Fields</strong>
        <button type="button" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <FieldsList fields={fields} onToggle={onToggle} onReorder={onReorder} />
      <p className="popover-hint">
        Drag a row to reorder it. The blue bar shows where it will land. Toggle the checkbox to
        show or hide a column.
      </p>
    </div>
  );
}

function FieldsList({
  fields,
  onToggle,
  onReorder,
}: {
  fields: ManagedField[];
  onToggle: (index: number) => void;
  onReorder: (from: number, to: number) => void;
}) {
  const [dropIndicator, setDropIndicator] = useState<{ index: number; before: boolean } | null>(
    null,
  );

  if (fields.length === 0) {
    return (
      <ul className="fields-list">
        <li className="empty">(no fields seen yet)</li>
      </ul>
    );
  }

  return (
    <ul className="fields-list">
      {fields.map((field, i) => {
        const isBefore = dropIndicator?.index === i && dropIndicator.before;
        const isAfter = dropIndicator?.index === i && !dropIndicator.before;
        return (
          <li
            key={field.name}
            draggable
            className={[isBefore && "drop-before", isAfter && "drop-after"]
              .filter(Boolean)
              .join(" ")}
            onDragStart={(ev) => {
              ev.dataTransfer.effectAllowed = "move";
              ev.dataTransfer.setData("text/plain", String(i));
              ev.currentTarget.classList.add("dragging");
            }}
            onDragEnd={(ev) => {
              ev.currentTarget.classList.remove("dragging");
              setDropIndicator(null);
            }}
            onDragOver={(ev) => {
              ev.preventDefault();
              ev.dataTransfer.dropEffect = "move";
              const rect = ev.currentTarget.getBoundingClientRect();
              const before = ev.clientY < rect.top + rect.height / 2;
              setDropIndicator({ index: i, before });
            }}
            onDragLeave={(ev) => {
              const next = ev.relatedTarget as Node | null;
              if (next && ev.currentTarget.contains(next)) return;
              setDropIndicator((curr) => (curr?.index === i ? null : curr));
            }}
            onDrop={(ev) => {
              ev.preventDefault();
              const from = Number(ev.dataTransfer.getData("text/plain"));
              const rect = ev.currentTarget.getBoundingClientRect();
              const before = ev.clientY < rect.top + rect.height / 2;
              let to = i + (before ? 0 : 1);
              if (from < to) to -= 1;
              setDropIndicator(null);
              if (Number.isFinite(from) && from !== to) onReorder(from, to);
            }}
          >
            <span className="drag-handle">≡</span>
            <input
              type="checkbox"
              checked={field.visible}
              onChange={() => onToggle(i)}
            />
            <span className="field-name">{field.name}</span>
            {(field.from.length > 1 || field.from[0] !== field.name) && (
              <span className="field-from">(from: {field.from.join(", ")})</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
