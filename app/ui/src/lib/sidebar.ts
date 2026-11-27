// Barre latérale façon Arc : deux états mémorisés (déployée / réduite) et, en mode réduit,
// un aperçu flottant au survol du bord gauche. Logique pure (minuteries injectées) : testée seule.

export type SidebarMode = 'expanded' | 'collapsed';
/** `hidden` : rien ; `arming` : la souris attend sur le bord ; `shown` : panneau flottant visible. */
export type PeekPhase = 'hidden' | 'arming' | 'shown';

export const STORAGE_KEY = 'steno.sidebar.collapsed';
/** Délai de survol du bord avant l'aperçu. */
export const PEEK_DELAY = 120;
/** Délai avant de rétracter l'aperçu une fois la souris sortie. */
export const PEEK_HIDE_DELAY = 250;

type StorageLike = Pick<Storage, 'getItem' | 'setItem'>;

/** État mémorisé ; toute erreur d'accès au stockage (navigation privée, bac à sable) vaut « déployée ». */
export function readCollapsed(storage: StorageLike | null | undefined): boolean {
  try {
    return storage?.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export function writeCollapsed(storage: StorageLike | null | undefined, collapsed: boolean): void {
  try {
    storage?.setItem(STORAGE_KEY, collapsed ? '1' : '0');
  } catch {
    // Stockage indisponible : l'état ne sera simplement pas mémorisé.
  }
}

export interface Timers {
  set(fn: () => void, ms: number): unknown;
  clear(id: unknown): void;
}

export type SidebarState = { mode: SidebarMode; peek: PeekPhase };

export interface SidebarOptions {
  collapsed: boolean;
  timers: Timers;
  onChange?: (s: SidebarState) => void;
  /** Appelé à chaque changement durable (déployée / réduite) : mémorisation. */
  persist?: (collapsed: boolean) => void;
}

export class SidebarController {
  mode: SidebarMode;
  peek: PeekPhase = 'hidden';
  /** La souris est sur le bord sensible ou sur le panneau flottant. */
  private pointer = { edge: false, panel: false };
  /** Focus clavier ou champ actif dans le panneau : il reste ouvert. */
  private held = false;
  private timer: unknown = null;
  private readonly t: Timers;
  private readonly onChange?: (s: SidebarState) => void;
  private readonly persist?: (collapsed: boolean) => void;

  constructor(o: SidebarOptions) {
    this.mode = o.collapsed ? 'collapsed' : 'expanded';
    this.t = o.timers;
    this.onChange = o.onChange;
    this.persist = o.persist;
  }

  get collapsed(): boolean {
    return this.mode === 'collapsed';
  }
  /** La barre est visible (déployée, ou en aperçu). */
  get visible(): boolean {
    return this.mode === 'expanded' || this.peek === 'shown';
  }
  get state(): SidebarState {
    return { mode: this.mode, peek: this.peek };
  }

  private emit() {
    this.onChange?.(this.state);
  }
  private cancel() {
    if (this.timer !== null) this.t.clear(this.timer);
    this.timer = null;
  }
  private schedule(fn: () => void, ms: number) {
    this.cancel();
    this.timer = this.t.set(() => {
      this.timer = null;
      fn();
    }, ms);
  }
  private setPeek(p: PeekPhase) {
    if (this.peek === p) return;
    this.peek = p;
    this.emit();
  }
  private get inside() {
    return this.pointer.edge || this.pointer.panel;
  }
  private maybeHide() {
    if (this.peek !== 'shown' || this.inside || this.held) return;
    this.schedule(() => this.setPeek('hidden'), PEEK_HIDE_DELAY);
  }

  // ---- États durables ----

  setMode(mode: SidebarMode) {
    this.cancel();
    this.pointer = { edge: false, panel: false };
    this.held = false;
    const changed = mode !== this.mode || this.peek !== 'hidden';
    this.mode = mode;
    this.peek = 'hidden';
    this.persist?.(mode === 'collapsed');
    if (changed) this.emit();
  }
  collapse() {
    this.setMode('collapsed');
  }
  expand() {
    this.setMode('expanded');
  }
  /** Bascule (bouton, ⌘\) : depuis l'aperçu, la barre est épinglée. */
  toggle() {
    this.setMode(this.mode === 'expanded' ? 'collapsed' : 'expanded');
  }
  /** Épingle l'aperçu : la barre redevient déployée durablement. */
  pin() {
    this.expand();
  }

  // ---- Aperçu au survol ----

  edgeEnter() {
    if (this.mode !== 'collapsed') return;
    this.pointer.edge = true;
    if (this.peek === 'shown') {
      this.cancel();
      return;
    }
    if (this.peek === 'arming') return;
    this.setPeek('arming');
    this.schedule(() => this.setPeek('shown'), PEEK_DELAY);
  }
  edgeLeave() {
    this.pointer.edge = false;
    if (this.peek === 'arming') {
      this.cancel();
      this.setPeek('hidden');
      return;
    }
    this.maybeHide();
  }
  panelEnter() {
    if (this.mode !== 'collapsed' || this.peek !== 'shown') return;
    this.pointer.panel = true;
    this.cancel();
  }
  panelLeave() {
    this.pointer.panel = false;
    this.maybeHide();
  }
  /** Focus clavier, menu ou champ actif dans le panneau. */
  hold(on: boolean) {
    this.held = on && this.mode === 'collapsed';
    if (this.held) this.cancel();
    else this.maybeHide();
  }
  /** Ouvre l'aperçu tout de suite (clavier, tests). */
  show() {
    if (this.mode !== 'collapsed') return;
    this.cancel();
    this.setPeek('shown');
  }
  /** Referme l'aperçu tout de suite (navigation, Échap, palette). */
  dismiss() {
    this.cancel();
    this.pointer = { edge: false, panel: false };
    this.held = false;
    this.setPeek('hidden');
  }
}
