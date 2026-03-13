export type ToastType = "error" | "warning" | "success" | "info";

interface Toast {
  id: number;
  type: ToastType;
  message: string;
}

let toasts = $state<Toast[]>([]);
let nextId = 0;

export function getToasts(): Toast[] {
  return toasts;
}

export function addToast(type: ToastType, message: string): void {
  // Dedup: skip if an identical message is already showing
  if (toasts.some((t) => t.message === message)) return;

  const id = nextId++;
  toasts = [...toasts, { id, type, message }];

  // Auto-dismiss non-error toasts after 5s
  if (type !== "error") {
    setTimeout(() => dismiss(id), 5000);
  }
}

export function dismiss(id: number): void {
  toasts = toasts.filter((t) => t.id !== id);
}

export function toastError(message: string): void {
  addToast("error", message);
}

export function toastWarning(message: string): void {
  addToast("warning", message);
}

export function toastSuccess(message: string): void {
  addToast("success", message);
}

export function toastInfo(message: string): void {
  addToast("info", message);
}
