<script lang="ts" module>
  // Icônes au trait (24×24, trait arrondi), dans le style du logo.
  const P: Record<string, string> = {
    meetings: 'M7 3.5h7.5L19 8v12.5H7z M14.5 3.5V8H19 M10 12h6 M10 15.5h6',
    live: 'M4 10v4 M8 7v10 M12 4v16 M16 7v10 M20 10v4',
    search: 'M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13z M15.5 15.5 20 20',
    actions: 'M4.5 6.5l1.6 1.6 3-3 M4.5 13l1.6 1.6 3-3 M12 7h8 M12 13.5h8 M12 19.5h8 M5 19.5h2',
    settings: 'M4 7h9 M17 7h3 M4 17h3 M11 17h9 M15 4.5v5 M9 14.5v5',
    sidebar: 'M4.5 5.5h15a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z M9.5 5.5v13 M6 9h1.5 M6 11.5h1.5',
    pin: 'M9.5 4.5h5 M10.5 4.5v5l-3.5 4h10l-3.5-4v-5 M12 13.5v6',
    laptop: 'M5.5 6.5a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1v8h-13z M3 17.5h18 M3.5 14.5h17l.5 3h-18z',
    bluetooth: 'M7 7.5l10 9-5 4v-17l5 4-10 9',
    usb: 'M12 3.5v14 M9.5 6L12 3.5 14.5 6 M12 14l-4-2.5V9 M12 12l4-2V7.5 M7 8h2v1.5H7z M15 6h2v1.5h-2z M12 20.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z',
    terminal: 'M4.5 5.5h15a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z M7.5 9.5l2.5 2.5-2.5 2.5 M12.5 14.5h4',
    command: 'M9 9V7a2 2 0 1 0-2 2z M15 9h2a2 2 0 1 0-2-2z M9 15H7a2 2 0 1 0 2 2z M15 15v2a2 2 0 1 0 2-2z M9 9h6v6H9z',
    enter: 'M18.5 5.5v6a3 3 0 0 1-3 3h-10 M9 10.5l-4 4 4 4',
    flag: 'M6 20.5v-16 M6 5h11.5l-2.5 4 2.5 4H6',
    arrowright: 'M5 12h14 M13.5 6.5 19 12l-5.5 5.5',
    checkcircle: 'M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M8.5 12.2l2.4 2.4 4.6-4.8',
    mic: 'M12 3.5a2.8 2.8 0 0 1 2.8 2.8v5.4a2.8 2.8 0 0 1-5.6 0V6.3A2.8 2.8 0 0 1 12 3.5z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3.5',
    micoff: 'M12 3.5a2.8 2.8 0 0 1 2.8 2.8v5.4a2.8 2.8 0 0 1-5.6 0V6.3A2.8 2.8 0 0 1 12 3.5z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3.5 M4 4l16 16',
    speaker: 'M4.5 9.5h3.5L12.5 5.5v13l-4.5-4H4.5z M16 9a4 4 0 0 1 0 6 M18.5 6.5a7.5 7.5 0 0 1 0 11',
    play: 'M8 5.5v13l10-6.5z',
    stop: 'M7 7h10v10H7z',
    pause: 'M8.5 6v12 M15.5 6v12',
    copy: 'M9 9h10.5v10.5H9z M15 9V4.5H4.5V15H9',
    download: 'M12 4v11 M7.5 10.5 12 15l4.5-4.5 M5 19.5h14',
    printer: 'M7 9V4h10v5 M7 17H4.5V9.5h15V17H17 M7 14h10v6H7z',
    share: 'M12 4v11 M7.5 8.5 12 4l4.5 4.5 M5 13.5v6h14v-6',
    refresh: 'M19 12a7 7 0 1 1-2.05-4.95 M19 4.5v4h-4',
    link: 'M10 14a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1 1 M14 10a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1-1',
    trash: 'M5 7h14 M10 7V4.5h4V7 M7 7l.8 12.5h8.4L17 7',
    pencil: 'M15.5 5l3.5 3.5L9 18.5H5.5V15z',
    lock: 'M6.5 11h11v8.5h-11z M8.5 11V8a3.5 3.5 0 0 1 7 0v3',
    chevron: 'M9 6l6 6-6 6',
    plus: 'M12 5v14 M5 12h14',
    x: 'M6 6l12 12 M18 6 6 18',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    ask: 'M12 3.5l1.8 4.7 4.7 1.8-4.7 1.8L12 16.5l-1.8-4.7L5.5 10l4.7-1.8z M18 16l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z',
    hand: 'M8 12V6.5a1.5 1.5 0 0 1 3 0V11 M11 10.5V5a1.5 1.5 0 0 1 3 0v5.5 M14 10.5V6.5a1.5 1.5 0 0 1 3 0v6.5a6.5 6.5 0 0 1-6.5 6.5c-2.5 0-3.8-1.2-5.2-3.4L4 13.4a1.4 1.4 0 0 1 2.3-1.6L8 14',
    external: 'M13.5 4.5h6v6 M19.5 4.5 11 13 M17 14v5.5H4.5V7H10',
    warn: 'M12 4 21 19.5H3z M12 10v4.5 M12 17.2v.1',
    info: 'M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M12 11v5.5 M12 7.8v.1',
    eye: 'M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z M12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5z',
    folder: 'M3.5 6.5h6l2 2h9v10h-17z',
    plug: 'M9 3.5v4 M15 3.5v4 M6.5 7.5h11v3a5.5 5.5 0 0 1-11 0z M12 16v4.5',
    agent: 'M12 3.5v3 M6 6.5h12v11H6z M9.5 11v1 M14.5 11v1 M9.5 14.5h5 M3.5 11v3 M20.5 11v3',
    link2: 'M8 12h8 M9 7H7a5 5 0 0 0 0 10h2 M15 7h2a5 5 0 0 1 0 10h-2',
    wave: 'M3 12h2l2-5 3 10 3-14 3 12 2-3h3',
    shield: 'M12 3.5l7 2.5v5.5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z',
    user: 'M12 4a3.8 3.8 0 1 1 0 7.6A3.8 3.8 0 0 1 12 4z M5 20c.8-3.8 3.6-5.5 7-5.5s6.2 1.7 7 5.5',
    swap: 'M7 4.5 4 7.5l3 3 M4 7.5h13 M17 13.5l3 3-3 3 M20 16.5H7',
    dots: 'M6 12h.01 M12 12h.01 M18 12h.01',
    paperclip: 'M15.5 7.5l-6.8 6.8a2 2 0 0 0 2.8 2.8l7-7a4 4 0 0 0-5.7-5.7l-7 7a6 6 0 0 0 8.5 8.5l6.3-6.3',
    file: 'M6.5 3.5h7l4 4v13h-11z M13.5 3.5v4h4',
    note: 'M5 4.5h14v15H5z M8.5 9h7 M8.5 12.5h7 M8.5 16h4',
    calendar: 'M4.5 6.5h15v13h-15z M4.5 10.5h15 M8.5 4v4 M15.5 4v4',
    clock: 'M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M12 7.5V12l3 2',
    users: 'M9 4.5a3.3 3.3 0 1 1 0 6.6 3.3 3.3 0 0 1 0-6.6z M3 19.5c.6-3.3 3-5 6-5s5.4 1.7 6 5 M15.5 4.8a3.2 3.2 0 0 1 0 6.1 M17.5 14.8c1.8.6 3 2.2 3.5 4.7',
    arrowdown: 'M12 5v14 M6.5 13.5 12 19l5.5-5.5',
    send: 'M4.5 12h11 M11 6.5l5.5 5.5-5.5 5.5',
    phone: 'M7.2 4.5h2.3l1.4 3.6-1.9 1.3a9.5 9.5 0 0 0 5.6 5.6l1.3-1.9 3.6 1.4v2.3a1.9 1.9 0 0 1-2.1 1.9C10.6 18.2 5.8 13.4 5.3 6.6a1.9 1.9 0 0 1 1.9-2.1z',
  };
  export type IconName = keyof typeof P;
</script>

<script lang="ts">
  let { name, size = 16, stroke = 1.6 }: { name: string; size?: number; stroke?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={name === 'dots' ? 3 : stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class="icon"
><path d={P[name] ?? ''} /></svg>

<style>
  .icon {
    display: inline-block;
    flex: none;
    vertical-align: -0.2em;
  }
</style>
