// Method/lesson catalogue — navigation only, no pCloud calls (all via IPC).
import { listMethods, saveMethod, scanMethod } from "../../lib/ipc";
import type { Lesson, Method } from "../../types/model";

export class Catalogue {
  private readonly container: HTMLElement;
  private methods: Method[] = [];

  /** Called when the user selects a lesson. */
  onLessonSelect: ((lesson: Lesson, method: Method) => void) | null = null;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  async load(): Promise<void> {
    this.methods = await listMethods();
    this.render();
  }

  /** Trigger a pCloud folder scan and persist the resulting Method. */
  async addMethod(rootFolderId: number, title: string): Promise<void> {
    const method = await scanMethod(rootFolderId, title);
    await saveMethod(method);
    this.methods.push(method);
    this.render();
  }

  private render(): void {
    this.container.innerHTML = "";
    for (const method of this.methods) {
      const section = document.createElement("section");
      const h2 = document.createElement("h2");
      h2.textContent = method.title;
      section.appendChild(h2);

      const ol = document.createElement("ol");
      for (const lesson of [...method.lessons].sort((a, b) => a.order - b.order)) {
        const li = document.createElement("li");
        li.textContent = lesson.title;
        li.style.cursor = "pointer";
        li.addEventListener("click", () => this.onLessonSelect?.(lesson, method));
        ol.appendChild(li);
      }
      section.appendChild(ol);
      this.container.appendChild(section);
    }
  }
}
