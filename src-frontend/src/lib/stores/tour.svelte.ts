export interface TourStep {
  target: string | null;
  description: string;
  placement: "top" | "bottom" | "left" | "right" | "center";
}

let active = $state(false);
let stepIndex = $state(0);
let steps = $state<TourStep[]>([]);
let tourStarted = false;

const STORAGE_KEY = "noren:tour_completed";

export function isActive(): boolean {
  return active;
}

export function getStepIndex(): number {
  return stepIndex;
}

export function getSteps(): TourStep[] {
  return steps;
}

export function getCurrentStep(): TourStep | null {
  return active ? steps[stepIndex] ?? null : null;
}

export function startTour(tourSteps: TourStep[]): void {
  if (tourStarted) return;
  if (localStorage.getItem(STORAGE_KEY)) return;
  tourStarted = true;
  steps = tourSteps;
  stepIndex = 0;
  active = true;
}

export function nextStep(): void {
  if (stepIndex < steps.length - 1) {
    stepIndex++;
  } else {
    completeTour();
  }
}

export function skipTour(): void {
  completeTour();
}

function completeTour(): void {
  active = false;
  stepIndex = 0;
  localStorage.setItem(STORAGE_KEY, "1");
}
