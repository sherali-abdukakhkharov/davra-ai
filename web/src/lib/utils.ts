import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const REPO_URL = "https://github.com/sherali-abdukakhkharov/ai-team";
export const DOCS_URL = `${REPO_URL}#readme`;
export const HUB_URL = "https://davra.qitmir.uz";
