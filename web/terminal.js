// A VT100 on one console, and the key encoding the plain console box uses.
//
// xterm.js does the terminal work — cursor movement, colours, line editing, full-screen
// programs — and encodes its own keys. It is fetched, not committed: tools/fetch-web-vendor.sh
// puts it in web/vendor/xterm, pinned by version and hash. 100×30, fixed, so `stty cols 100
// rows 30` on the guest describes it exactly.
window.installTerminal = function (container, send, { src = 'uart0', cols = 100, rows = 30 } = {}) {
  let term = null;
  return {
    src,
    // First show creates the terminal and replays `backlog` (the lines that arrived before,
    // colours already stripped) and `partial` (the unterminated last line: a prompt).
    show(backlog, partial) {
      if (!term) {
        if (!window.Terminal) { container.innerHTML = '<span style="color:#9aa">xterm.js is not here — run <code>tools/fetch-web-vendor.sh</code> once (the Pages build does); the plain UART0 tab works without it</span>'; return; }
        term = new Terminal({ cols, rows, fontSize: 12, fontFamily: 'ui-monospace,Menlo,monospace', cursorBlink: true, scrollback: 5000, theme: { background: '#0b0d10', foreground: '#e6e6e6', cursor: '#e6e6e6' } });
        term.open(container);
        term.onData(d => send({ t: 'key', src, data: d }));
        for (const text of backlog) term.write(text + '\r\n');
        if (partial) term.write(partial);
      }
      term.focus();
    },
    write(data) { if (term) term.write(data); },   // a no-op until shown: the backlog covers it
  };
};

// The bytes a terminal sends for a key event, or null when the browser should keep the key.
window.terminalKeyBytes = function (e) {
  if (e.metaKey || e.altKey) return null;   // the browser's own shortcuts stay the browser's
  const esc = { ArrowUp: '\x1b[A', ArrowDown: '\x1b[B', ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D', Home: '\x1b[H', End: '\x1b[F', Delete: '\x1b[3~', PageUp: '\x1b[5~', PageDown: '\x1b[6~', Insert: '\x1b[2~', F1: '\x1bOP', F2: '\x1bOQ', F3: '\x1bOR', F4: '\x1bOS' };
  if (e.key === 'Enter') return '\r'; if (e.key === 'Backspace') return '\x7f'; if (e.key === 'Tab') return '\t'; if (e.key === 'Escape') return '\x1b';
  if (esc[e.key]) return esc[e.key];
  if (e.key.length !== 1) return null;
  if (e.ctrlKey) { const c = e.key.toLowerCase(); if (c >= 'a' && c <= 'z') return String.fromCharCode(c.charCodeAt(0) - 96); if (c === '[') return '\x1b'; if (c === ' ') return '\0'; return null; }
  return e.key;
};
