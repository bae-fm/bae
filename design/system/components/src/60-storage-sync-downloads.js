  // ==== part: storage ====

  // ---- Stroke symbols for the parts below (SF Symbols and Material icons, drawn as lines) ----
  var SYM = {
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    pin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9',
    unpin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9M4 4l16 16',
    cloud: 'M7 18a4 4 0 0 1-.5-8A6 6 0 0 1 18 9a4.5 4.5 0 0 1 0 9z',
    up: 'M12 20V5M6 11l6-6 6 6',
    down: 'M12 4v15M6 13l6 6 6-6',
    pause: 'M8 5v14M16 5v14',
    play: 'M7 4l13 8-13 8z',
    sidebar: 'M3 5h18v14H3zM15 5v14',
    clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2',
    xcircle: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9 9l6 6M15 9l-6 6',
    copy: 'M8 8h12v12H8zM4 16V4h12',
    left: 'M15 5l-7 7 7 7',
    right: 'M9 5l7 7-7 7',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5',
    photo: 'M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15 9h.01',
    globe: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18',
    sync: 'M4 12a8 8 0 0 1 14-5.3M20 4v4h-4M20 12a8 8 0 0 1-14 5.3M4 20v-4h4',
    moveToCloud: 'M12 4v12M7 11l5 5 5-5M5 20h14',
    external: 'M7 17L17 7M9 7h8v8',
    export: 'M12 15V3M7 8l5-5 5 5M5 12v8h14v-8',
    saveAs: 'M12 3v12M7 10l5 5 5-5M5 12v8h14v-8',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
    expand: 'M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7',
    close: 'M6 6l12 12M18 6L6 18',
    back: 'M20 12H5M11 6l-6 6 6 6',
    more: 'M12 5h.01M12 12h.01M12 19h.01',
    check: 'M5 12l5 5 9-10',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
    edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4'
  };
  function Sym(props) {
    var s = Number(props.size || 14);
    return h('svg', { width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true, className: cx('bae-sym', props.className), fill: 'none', stroke: 'currentColor', strokeWidth: props.weight || 2, strokeLinecap: 'round', strokeLinejoin: 'round', style: props.style }, h('path', { d: SYM[props.name] || '' }));
  }
  function list(s) { return String(s || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean); }

  // A grey bar where text or a picture will be while it loads: `width` and `height` in points, `radius` for a picture's corners (a bar's corners otherwise).
  // A grey bar where text or a picture will be: both platforms draw it alike, in their own units.
  function placeholderLine(props) {
    var st = { width: Number(props.width), height: Number(props.height) };
    if (props.radius != null) st.borderRadius = measure(props.radius);
    if (props.fixed) st.flex = 'none';
    return h('div', { className: 'bae-ph', style: st });
  }
  function MacPlaceholderLine(props) { return placeholderLine(props); }

  // ---- Storage Manager (macOS) ----
  // The Storage Manager's toolbar, past the traffic lights: the window's title, All / Local / Cloud (`filter`, All unless given), the Filter field (`search`, the text typed) and the Inspector toggle (`inspector`, on while the inspector shows). `disabled` (no library open): the filter, the search field and the inspector button are off, each drawn by its own off state.
  function MacStorageToolbar(props) {
    return h('header', { className: 'bae-m-storewin-bar' },
      h('span', { className: 'bae-m-storewin-title' }, 'Storage Manager'),
      h('span', { style: { flex: 1 } }),
      h(MacSegmentedControl, { options: 'All, Local, Cloud', selected: props.filter || 'All', off: props.disabled }),
      h('span', { style: { flex: 1 } }),
      h(MacSearchField, { kind: 'filter', label: 'Filter', value: props.search, off: props.disabled }),
      // The Inspector toggle is a native toolbar item: macOS gives it a box 36pt wide and as tall as the bar, 4pt from the window's edge, and VoiceOver reads that box as the button; the 28pt icon button sits at its centre.
      h('span', { className: 'bae-m-storewin-item', role: 'button', 'aria-label': 'Inspector', 'aria-disabled': props.disabled ? true : null },
        h('span', { 'aria-hidden': true }, h(MacIconButton, { icon: 'sidebar', size: 'medium', label: 'Inspector', active: props.inspector, off: props.disabled }))));
  }
  // The Storage Manager's window, its own frame as in the app: the toolbar, then the release list and, while `inspector` is on, the inspector. The list is the MacTable with the MacStorageFooter under it (or, with no library open, the first child alone); the other children are the inspector's. `empty`: the filter matches nothing, and the window says No matches over the table.
  function MacStorageWindow(props) {
    var isType = function (c, t) { var p = inner(c); return p && p.type === t; };
    // A sheet or an alert over the window is drawn last, as the window's own child, so its scrim covers the whole window.
    var isModal = function (c) { var p = inner(c); return p && (p.type === MacAlert || p.type === MacModal || p.type === MacSidePause || (p.type === MacSheet && !!p.props.overlay)); };
    var all = kids(props.children);
    var k = all.filter(function (c) { return !isModal(c); });
    var table = k.filter(function (c) { return isType(c, MacTable); });
    var list = table.length
      ? [h('div', { key: 't', className: 'bae-m-storewin-table' }, table, props.empty ? h('div', { className: 'bae-m-storewin-empty' }, h(MacContentUnavailable, { title: 'No matches' })) : null)].concat(k.filter(function (c) { return isType(c, MacStorageFooter); }))
      : k.slice(0, 1);
    var rest = table.length ? k.filter(function (c) { return !isType(c, MacTable) && !isType(c, MacStorageFooter); }) : k.slice(1);
    return h('div', { className: 'bae-m-storewin', style: { width: Number(props.width || 1100), height: Number(props.height || 660) } }, macLights(),
      h(MacStorageToolbar, { filter: props.filter, search: props.search, inspector: props.inspector, disabled: props.disabled }),
      h('div', { className: 'bae-m-storewin-body' },
        h('div', { className: 'bae-m-storewin-list' }, list),
        props.inspector ? h('div', { className: 'bae-m-storewin-inspector' }, rest) : null),
      all.filter(isModal));
  }
  // A table's columns: "Title:width[:right][:up|:down]", the first taking the room the others leave; `up` or `down` marks the column the table sorts by.
  function cols(s) { return list(s).map(function (c) { var p = c.split(':'); return { title: p[0], w: Number(p[1] || 100), right: p.indexOf('right') > 1, sort: p.indexOf('up') > 1 ? 'up' : p.indexOf('down') > 1 ? 'down' : null }; }); }
  // Entries: one row a ";"-separated entry, one cell a "|"-separated field.
  function entries(s) { return String(s || '').split(';').map(function (x) { return x.trim(); }).filter(Boolean).map(function (r) { return r.split('|').map(function (x) { return x.trim(); }); }); }
  function tableHead(cs) {
    return h('div', { className: 'bae-m-table-head' }, cs.map(function (c, i) { return h(c.plain ? 'span' : 'button', { key: i, type: c.plain ? null : 'button', className: cx('bae-m-table-th', c.right && 'bae-m-table-right') }, c.title, c.sort ? h(Glyph, { set: 'mac', name: 'down', size: 'badge', className: 'bae-muted' }) : null); }));
  }
  function tableTemplate(cs) { return cs.map(function (c, i) { return i === 0 ? 'minmax(' + c.w + 'px, 1fr)' : c.w + 'px'; }).join(' '); }
  // The Storage Manager's release table, AppKit's NSTableView: the header (`columns`), then one row a release, every other row on the hover fill, the `selected` rows (their numbers, from 1, separated by commas) in the accent's tint. `rows` holds the releases, one entry a row with a field a column in the columns' order: the release's title, its artist, media, storage (a MacStorageChip state, and its progress for a state that moves: "uploading 0.62"), files, size and date added. The Release column draws MacReleaseCell and every other column MacStorageCell. `loading`: that many rows still on their way after the given ones, each a placeholder bar in its Release column.
  function MacTable(props) {
    var cs = cols(props.columns);
    var sel = list(props.selected).map(Number);
    var cell = function (c, v) {
      if (c.title === 'Release') return h(MacReleaseCell, { title: v });
      if (c.title === 'Storage') { var st = String(v || '').split(' '); return h(MacStorageCell, { state: st[0], progress: st[1] }); }
      return h(MacStorageCell, { value: v, numeric: c.title === 'Files' || c.title === 'Size' });
    };
    var rows = entries(props.rows).map(function (r, i) {
      return h('div', { key: i, className: cx('bae-m-table-tr', i % 2 && 'bae-m-table-alt') },
        h('div', { className: cx('bae-m-table-row', sel.indexOf(i + 1) >= 0 && 'bae-m-table-selected') }, cs.map(function (c, j) { return h('span', { key: j, className: 'bae-m-table-td' }, cell(c, r[j])); })));
    });
    for (var n = 0; n < Number(props.loading || 0); n++) {
      var k = rows.length;
      rows.push(h('div', { key: k, className: cx('bae-m-table-tr', k % 2 && 'bae-m-table-alt') },
        h('div', { className: 'bae-m-table-row' }, h('span', { className: 'bae-m-table-td' }, h(MacPlaceholderLine, { width: 180, height: 14, radius: 'radius-bar' })))));
    }
    return h('div', { className: 'bae-m-table', style: { '--bae-cols': tableTemplate(cs), minWidth: cs.reduce(function (n, c) { return n + c.w; }, 20) } }, tableHead(cs), h('div', { className: 'bae-m-table-body' }, rows));
  }
  // The Release column's cell: the release's 24pt cover beside its title.
  function MacReleaseCell(props) { return h('span', { className: 'bae-m-relcell' }, h(Artwork, { title: props.title, size: 24, radius: 'radius-bar' }), h('span', { className: 'bae-ellipsis' }, props.title)); }
  // Every other column's cell in the Storage Manager's table: its text (`value`: the artist, the media, the date added), a number at the column's end in tabular figures (`numeric`: files, size), or the storage column's MacStorageChip (`state`, with `progress` for a state that moves).
  function MacStorageCell(props) {
    if (props.state) return h(MacStorageChip, { state: props.state, progress: props.progress });
    return h('span', { className: props.numeric ? 'bae-num bae-m-storecell-num' : null }, props.value);
  }
  // A release's files in its Storage sheet: the Files heading over the columns Name, Audio, Size and Kind, the file name in the text colour, its audio format (empty for a file that isn't audio) and size in secondary text, its kind in tertiary. Name, Size and Kind sort the table, Audio doesn't; `sort` is the column it sorts by and its direction (Name:up unless given), marked in its header. `files` lists them, one "name|audio|size|kind" each, separated by semicolons.
  function MacReleaseFileTable(props) {
    var sort = String(props.sort || 'Name:up').split(':');
    var cs = cols('Name:200,Audio:120,Size:80,Kind:130').map(function (c) { if (c.title === sort[0]) c.sort = sort[1] || 'up'; c.plain = c.title === 'Audio'; return c; });
    var rows = entries(props.files).map(function (f, i) {
      return h('div', { key: i, className: cx('bae-m-table-tr', i % 2 && 'bae-m-table-alt') },
        h('div', { className: 'bae-m-table-row' },
          h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-ellipsis' }, f[0])), h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-muted' }, f[1] || '')),
          h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-muted bae-num' }, f[2])), h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-m-tertiary' }, f[3]))));
    });
    return h(React.Fragment, null,
      h('div', { style: { padding: 'var(--space-related) var(--space-edge) var(--space-inline)' } }, h(MacEyebrow, null, 'Files')),
      h('div', { style: { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 } },
        h('div', { className: 'bae-m-table', style: { '--bae-cols': tableTemplate(cs) } }, tableHead(cs), h('div', { className: 'bae-m-table-body' }, rows))));
  }
  // The Storage column, as MacStorageChip draws it: one word, with no icon. Every state that stands still is a plain word: Local, Pinned and Cloud at rest, Queued, and Waiting and Error in their tone's colour. Only the states with progress (Uploading, Pinning, Moving) are filled chips. Moving: Uploading, Pinning and Moving (to This Mac) fill with `progress`. A pin, upload or move to This Mac waiting its turn shows the chip Queued (`queued`, `uploadQueued`, `moveToThisDeviceQueued`). An upload held by a missing original shows the chip Waiting, in warning (`uploadBlocked`). A pin, move or move to This Mac that failed shows the chip Error, in danger (`downloadFailed`, `moveFailed`, `moveToThisDeviceFailed`). The inspector says what is queued or moving, what failed, and what an upload waits for. Nothing says how many wait ahead or what holds the queue back. `long`, for a release's Storage sheet (MacStorageBand), names what a waiting or moving change does, as everywhere but the column: Queued to upload, Queued to pin, Queued to move to This Mac, Moving to This Mac….
  function MacStorageChip(props) {
    var s = props.state || 'cloud';
    // A state that stands still is a word in the row's text; a problem's word takes its tone's colour. Only a state with progress is a filled chip.
    var word = function (tone, label) { return h('span', { className: cx('bae-m-storeword', tone && 'bae-tone-' + tone + ' bae-m-storeword-tone') }, label); };
    if (s === 'uploading' || s === 'pinning' || s === 'detaching') {
      var label = s === 'uploading' ? 'Uploading' : s === 'pinning' ? 'Pinning' : props.long ? 'Moving to This Mac…' : 'Moving';
      return h('span', { className: 'bae-m-progchip' }, h('span', { className: 'bae-m-progchip-fill', style: { width: (Number(props.progress || 0.4) * 100) + '%' } }), h('span', { className: 'bae-m-progchip-label' }, label));
    }
    if (s === 'downloadFailed' || s === 'moveFailed' || s === 'moveToThisDeviceFailed') return word('danger', 'Error');
    if (s === 'queued' || s === 'uploadQueued' || s === 'moveToThisDeviceQueued') return word(null, props.long ? { queued: 'Queued to pin', uploadQueued: 'Queued to upload', moveToThisDeviceQueued: 'Queued to move to This Mac' }[s] : 'Queued');
    if (s === 'uploadBlocked') return word('warning', 'Waiting');
    return word(null, { local: 'Local', pinned: 'Pinned', cloud: 'Cloud' }[s]);
  }

  // A release's transfer that stopped, at the top of the inspector: the kind's icon, title and detail, the cancel mark; under them what happened (`notice`, with `subtitle`) and its buttons. A pin (`pin`) or a move to This Mac (`moveToThisDevice`) that failed shows the notice Could not pin or Could not move to This Mac, with Details and Retry; a move to the cloud (`upload`) names its file in `subtitle` and gives its own `tone` and `action`.
  function MacFailedTransferRow(props) {
    var kind = props.kind || 'pin';
    var icon = { moveToThisDevice: 'folder', upload: 'up', pin: 'pin' }[kind];
    var notice = props.notice || (kind === 'moveToThisDevice' ? 'Could not move to This Mac' : 'Could not pin');
    return h('div', { className: 'bae-m-dlrow' },
      h(MacItem, { lead: h(Sym, { name: icon, size: 16, className: 'bae-muted' }), trail: h('button', { type: 'button', className: 'bae-plainbtn bae-muted', 'aria-label': 'Cancel', title: 'Cancel' }, h(Sym, { name: 'xcircle', size: 15 })) },
        h('div', { className: 'bae-m-dlrow-text' }, h('span', { className: 'bae-ellipsis' }, props.title || 'Glass Harbor'), h('span', { className: 'bae-muted bae-num bae-m-dlrow-detail' }, props.detail || '9 files · 356 MB'))),
      h(MacNotice, { quiet: true, tone: props.tone || 'danger', title: notice, subtitle: props.subtitle, secondary: props.secondary === undefined ? 'Details' : props.secondary || null, action: props.action || 'Retry', buttons: props.subtitle ? 'below' : null }));
  }

  // One transfer queue under the Storage Manager's list (`kind`: uploads, pins or exports): its arrow, then its rate (`rate`) or in its place what holds the whole queue back (`wait`: Paused, Waiting for network, Waiting for Wi-Fi, Retrying at 6:40 PM), and Pause, or Resume while the person has paused it (`wait` Paused).
  var QUEUE_PAUSE = { uploads: 'Pause uploads', pins: 'Pause downloads', exports: 'Pause exports' };
  function MacQueueRate(props) {
    var kind = props.kind || 'uploads', wait = props.wait, paused = wait === 'Paused', label = QUEUE_PAUSE[kind];
    return h('span', { className: 'bae-m-storefoot-q' },
      h(Sym, { name: kind === 'exports' ? 'export' : kind === 'uploads' ? 'up' : 'down', size: 11, className: kind === 'exports' ? null : 'bae-accent' }),
      wait ? h('span', null, wait) : props.rate ? h('span', { className: 'bae-num' }, props.rate) : null,
      h('span', { title: paused ? label.replace('Pause', 'Resume') : label }, h(Sym, { name: paused ? 'play' : 'pause', size: 10 })));
  }
  // Under the list: how many releases, the total, then each queue with work (`uploads`, `pins`, `exports`: its rate) as MacQueueRate, with what holds it back (`uploadsWait`, `pinsWait`, `exportsWait`).
  function MacStorageFooter(props) {
    return h('div', { className: 'bae-m-storefoot' },
      h('span', null, props.releases || '8 releases', props.total ? ' · Total: ' + props.total : ''),
      h('span', { style: { flex: 1 } }),
      props.uploads ? h(MacQueueRate, { kind: 'uploads', rate: props.uploads, wait: props.uploadsWait }) : null,
      props.pins ? h(MacQueueRate, { kind: 'pins', rate: props.pins, wait: props.pinsWait }) : null,
      props.exports ? h(MacQueueRate, { kind: 'exports', rate: props.exports, wait: props.exportsWait }) : null);
  }
  // A release's upload or pin in the inspector: the counts line with Cancel, then bytes, rate and a bar.
  function MacTransferSummary(props) {
    var kind = props.kind || 'upload';
    var parts = list(props.counts);
    return h('div', { className: 'bae-m-tsum' },
      h('div', { className: 'bae-m-tsum-line' },
        h('span', { className: 'bae-m-tsum-counts' }, parts.map(function (p, i) { return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-muted' }, ' · ') : null, h('span', { className: i === 0 ? 'bae-strong bae-accent' : i === parts.length - 1 ? 'bae-muted' : null }, p)); })),
        h('span', { style: { flex: 1 } }), props.cancel === false ? null : h(MacButton, { size: 'small' }, 'Cancel')),
      props.bytes ? h(MacTransferReadout, { kind: kind, bytes: props.bytes, rate: props.rate, progress: props.progress }) : null);
  }
  // A transfer's bytes and rate over its 4pt bar, AppKit's own view, since both change many times a second: the bytes moved (`bytes`) at the start and the rate (`rate`, or for a move to This Mac the current file's bytes) at the end, in secondary tabular figures, then the bar filled to `progress` in the accent; `kind: 'pin'` names the bar Pinning, and an upload's is Uploading.
  function MacTransferReadout(props) {
    var pin = props.kind === 'pin';
    return h('div', { className: 'bae-m-readout' },
      h('div', { className: 'bae-m-readout-line bae-num' }, h('span', null, props.bytes), h('span', null, props.rate)),
      h('div', { className: 'bae-m-readout-bar' }, h(ProgressBar, { value: Number(props.progress || 0.3), label: pin ? 'Pinning' : 'Uploading' })));
  }
  var filenameGraphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
  // Each half can shrink toward the middle without cutting a character at the split.
  function fileRowName(name, color) {
    var line = name.replace(/\r\n|[\n\r\f\u0085\u2028\u2029]/g, ' ');
    var letters = Array.from(filenameGraphemes.segment(line), function (part) { return part.segment; });
    var middle = Math.ceil(letters.length / 2);
    return h('div', { className: cx('bae-m-filerow-name', color) },
      h('span', { className: 'bae-m-filerow-full-name' }, line),
      h('span', { 'aria-hidden': true, className: 'bae-m-filerow-start' }, letters.slice(0, middle).join('')),
      h('span', { 'aria-hidden': true, className: 'bae-m-filerow-end' }, letters.slice(middle).join('')));
  }
  // One file in the inspector: a mark for queued to upload (`queued`), to pin (`pin-queued`) or to move to This Mac (`move-queued`), all dashed, sending, pinning or moving to This Mac (`moving`, a ring filling), or the file a move stopped on (`error`, in danger) or an upload waits for (`waiting`, in warning); its name and size. The mark is the only thing on the row that says where the file stands, so it is an image named by those words.
  function MacFileRow(props) {
    var s = props.state || 'rest';
    var label = { queued: 'Queued to upload', 'pin-queued': 'Queued to pin', 'move-queued': 'Queued to move to This Mac', sending: 'Uploading', pinning: 'Pinning', moving: 'Moving to This Mac…', error: 'Could not move to the cloud', waiting: 'Waiting for a file' }[s];
    var bang = function (tone) { return h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', role: 'img', 'aria-label': label, style: { color: 'var(--' + tone + ')' } }, h('circle', { cx: 8, cy: 8, r: 6.5, fill: 'none', stroke: 'currentColor', strokeWidth: 1.5 }), h('path', { d: 'M8 4.6v4.2M8 11.2v.1', stroke: 'currentColor', strokeWidth: 1.75, strokeLinecap: 'round' })); };
    var queued = s === 'queued' || s === 'pin-queued' || s === 'move-queued';
    var download = s === 'pinning' || s === 'moving';
    var mark = s === 'rest' ? null : s === 'error' ? bang('danger') : s === 'waiting' ? bang('warning') :
      queued ? h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', role: 'img', 'aria-label': label, className: 'bae-muted' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 1.75, strokeDasharray: '2 2' })) :
      h(MacProgressRing, { value: props.progress ?? 0.4, label: label });
    return h('div', { className: cx('bae-m-filerow', props.selected && 'bae-m-filerow-selected') },
      h(MacItem, { lead: mark, trail: h('span', { className: 'bae-muted bae-num' }, props.size) },
        fileRowName(props.name, cx(queued && 'bae-muted', (s === 'sending' || download) && 'bae-accent'))));
  }
  // Label and value pairs in two columns, the labels in secondary text: a file's facts, a release's fields, an album's credits. `facts` is a list of [label, value] pairs, or text with one "Label: value" a pair, separated by semicolons; a pair with no value is left out. `numeric` names the labels whose values are numbers, drawn in tabular figures (Size).
  function MacFactsGrid(props) {
    var facts = typeof props.facts === 'string'
      ? props.facts.split(';').map(function (x) { var i = x.indexOf(':'); return [x.slice(0, i).trim(), x.slice(i + 1).trim()]; }).filter(function (f) { return f[0]; })
      : props.facts || [];
    var numeric = list(props.numeric);
    return h('div', { className: cx('bae-m-filedet-grid', props.wide && 'bae-m-filedet-grid-wide') }, facts.filter(function (f) { return f[1]; }).map(function (f) {
      return [h('span', { key: f[0] + 'l', className: 'bae-muted' }, f[0]), h('span', { key: f[0] + 'v', className: numeric.indexOf(f[0]) >= 0 ? 'bae-num' : null }, f[1])];
    }));
  }
  function MacFileDetails(props) {
    return h('div', { className: 'bae-m-filedet' },
      h('div', { className: 'bae-m-filedet-head' }, h('span', { className: 'bae-ellipsis' }, props.name), h('span', { className: 'bae-muted', title: 'Close file details' }, h(Sym, { name: 'xcircle', size: 15 }))),
      h(MacFactsGrid, { facts: [['Folder', props.folder], ['Kind', props.kind], ['Format', props.format], ['Size', props.size]], numeric: 'Size' }));
  }
  // An error's causes, one a line, in an inset panel with Copy in its corner, in an alert (`MacAlert`'s `details`).
  function MacErrorDetails(props) {
    return h('div', { className: 'bae-errdis-panel bae-errdis-panel-hover' }, h('div', { className: 'bae-errdis-chain' }, props.details), h('span', { className: 'bae-errdis-copy', title: 'Copy details' }, h(Sym, { name: 'copy', size: 11 })));
  }
  // macOS's notice: a glyph (`icon: false` drops it), a title and a subtitle saying what to do, then up to two buttons: `action`, the primary one (Retry), and `secondary` (Dismiss, or Details, which opens MacAlert with the error's causes in `details`: a notice never shows the error itself). The buttons sit on the title's line at the right, secondary first; `buttons: 'below'` puts them on a line under the text. `dismiss` adds an × at the end of that line. `layout: 'centered'` is a failed view.
  function MacNotice(props) {
    var tone = props.tone || 'neutral';
    var centered = props.layout === 'centered';
    // Centred, the notice is quiet words: its button's label is secondary text, so nothing in it is brighter than the message; Details is in the accent, as in every notice, apart from the grey title.
    var primary = props.action ? h(MacButton, { key: 'p', muted: centered, tinted: !props.quiet }, props.action) : null;
    var secondary = props.secondary ? h(MacButton, { key: 's', kind: 'borderless' }, props.secondary) : null;
    var dismiss = props.dismiss ? h('span', { key: 'x', className: 'bae-m-notice-dismiss' }, h(MacIconButton, { icon: 'close', size: 'medium', faint: true, label: 'Dismiss' })) : null;
    // `leave`: the label of the button that leaves a screen the notice fills (Cancel), first in the row, where it is the screen's only way back.
    var leave = centered && props.leave ? h(MacButton, { key: 'l', muted: true }, props.leave) : null;
    if (centered) {
      // The glyph sits beside the title only; a subtitle is its own smaller, grey line under them, so the two lines read as a heading and what to do.
      var title = h(React.Fragment, null,
        h('div', { className: 'bae-notice-centered-title' }, noticeGlyph(tone, props.icon, { centered: true }), h('div', { className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.title), props.children),
        props.subtitle ? h('div', { className: 'bae-notice-centered-sub' }, props.subtitle) : null);
      return NoticeBox(tone, props.icon, h('div', { className: 'bae-m-notice' }, title, primary || secondary || leave ? h('div', { className: 'bae-notice-centered-row' }, leave, secondary, primary) : null), { platform: 'bae-m-notice-box', quiet: props.quiet, centered: true, stage: props.stage });
    }
    // The buttons sit on the text's line at the right, secondary first; `buttons: 'below'` puts them on a line of their own under the text.
    var below = props.buttons === 'below';
    var words = h('div', { key: 'w', className: 'bae-m-notice-words' }, noticeWords(props));
    var buttons = primary || secondary ? h('div', { key: 'b', className: below ? 'bae-m-notice-below' : 'bae-m-notice-buttons' }, secondary, primary) : null;
    // One top row, aligned to the title's line: the words, inline buttons, the ×; `below` buttons are a second row.
    var top = h('div', { className: 'bae-m-notice-top' }, words, below ? null : buttons, dismiss);
    var body = h('div', { className: 'bae-m-notice' }, top, below ? buttons : null);
    // The glyph sits in the row piece's leading column, so a notice among rows shares their text edge.
    var glyph = noticeGlyph(tone, props.icon, { quiet: props.quiet });
    return NoticeBox(tone, false, glyph ? h(MacItem, { lead: glyph }, body) : body, { platform: 'bae-m-notice-box', quiet: props.quiet, centered: false, stage: props.stage });
  }

  // A popover hanging off the control that opened it. Placed with `top`, it hangs below the control, its arrow on its upper edge pointing up; placed with `bottom`, it opens above, its arrow on its lower edge pointing down. `arrow`: the arrow's middle from the right edge (40). `flush`: no padding, for content that pads itself.
  function MacPopover(props) {
    var above = props.bottom != null;
    return h('div', { className: cx('bae-m-popover', props.flush && 'bae-m-popover-flush'), style: { width: Number(props.width || 360), top: props.top != null ? Number(props.top) : null, bottom: above ? Number(props.bottom) : null, right: props.right != null ? Number(props.right) : null } },
      h('span', { className: cx('bae-m-popover-arrow', above && 'bae-m-popover-arrow-below'), style: props.arrow != null ? { right: Number(props.arrow) - 7 } : null }), h('div', { className: 'bae-m-popover-body' }, props.children));
  }
  // A transfer in the Activity panel: what it does (`label`), how far it has got (`count`: done of total, or what holds the queue), then a bar filled to `progress` (none: a moving bar, as syncing draws, which counts nothing). `pausable` ends the line in Pause, or Resume while `paused`; pinning and moving to This Mac are one queue, so both lines' buttons act on it together.
  function MacTransferLine(props) {
    return h('div', { className: 'bae-m-actline bae-m-actline-xfer' },
      h('div', { className: 'bae-m-actxfer' }, h('span', null, props.label), h('span', { className: 'bae-m-actxfer-end' }, props.count ? h('span', { className: 'bae-muted bae-num' }, props.count) : null,
        props.pausable ? h(MacIconButton, { icon: props.paused ? 'play' : 'pause', size: 'medium', small: true, boxed: true, label: props.paused ? 'Resume' : 'Pause' }) : null)),
      h(MacProgress, { name: props.label, value: props.progress }));
  }
  // The toolbar's Activity panel (BaeKit's SyncActivityList, grown to hold every problem): Needs attention lists what bae needs the person for, as notices, each with an × that dismisses it from the panel (the problem itself stays on the screen where the person fixes it): the caller's own (children), Could not read sync status (`error`, its title), and sync's own failure (`attention`, the error's title: signed out, a record it can't send) with Open sync settings. Nothing in the panel retries in place. Then Transfers, each a line of words, how far it has got and a bar: Syncing changes (no count, its bar moves), Uploading (`uploads`), Pinning (`pins`), Moving to This Mac (`movingToThisDevice`), each counted in releases done of the batch ("1 of 3 releases"), its bar filled by the bytes done; what holds the uploads or the download queue (`uploadsWait`, `downloadsWait`: Paused, Waiting for network; for uploads also Waiting for Wi-Fi, Retrying at a time) stands in place of the count. When nothing needs attention and nothing transfers, the panel says Everything is up to date.
  function MacActivity(props) {
    var sec = function (title, body) { return h(MacSection, { title: title }, h(MacBox, null, body)); };
    // Every row of the panel's boxes has the same inset; a row in the problems' box without a glyph starts on the problems' text edge (MacItem's empty leading column). Transfers is its own box, so its lines start at the box's inset.
    var problemsListed = !!(kids(props.children).length || props.error || props.attention);
    var edge = function (body) { return problemsListed ? h(MacItem, null, body) : body; };
    var row = function (key, body) { return h('div', { key: key, className: 'bae-m-actrow' }, body); };
    // `held` is the queue's pause state; syncing has none.
    var xfer = function (key, label, count, progress, held) {
      return row(key, h(MacTransferLine, { label: label, count: count, progress: progress, pausable: held != null, paused: held }));
    };
    var out = [];
    var problems = kids(props.children).map(function (p, i) { return row('c' + i, p); }).concat([
      props.error ? row('e', h(MacNotice, { quiet: true, dismiss: true, tone: 'danger', title: props.error })) : null,
      props.attention ? row('f', h(MacNotice, { quiet: true, dismiss: true, tone: 'warning', title: props.attention, action: 'Open sync settings', buttons: 'below' })) : null,
      ].filter(Boolean));
    if (problems.length) out.push(h(React.Fragment, { key: 'a' }, sec('Needs attention', problems)));
    if (props.syncing || props.uploads || props.pins || props.movingToThisDevice) out.push(h(React.Fragment, { key: 't' }, sec('Transfers', [
      props.syncing ? xfer('s', 'Syncing changes', null, null) : null,
      props.uploads ? xfer('u', 'Uploading', props.uploadsWait || props.uploads, props.uploadProgress != null ? props.uploadProgress : 0.4, props.uploadsWait === 'Paused') : null,
      props.pins ? xfer('p', 'Pinning', props.downloadsWait || props.pins, props.pinProgress != null ? props.pinProgress : 0.3, props.downloadsWait === 'Paused') : null,
      props.movingToThisDevice ? xfer('m', 'Moving to This Mac', props.downloadsWait || props.movingToThisDevice, props.moveToThisDeviceProgress != null ? props.moveToThisDeviceProgress : 0.2, props.downloadsWait === 'Paused') : null].filter(Boolean))));
    // Under Transfers, Open storage manager for the detail.
    if (props.uploads || props.pins || props.movingToThisDevice) out.push(h('div', { key: 'k', className: 'bae-m-actfoot' }, h(MacButton, { size: 'small' }, 'Open storage manager')));
    if (!out.length) out.push(h(React.Fragment, { key: 'i' }, sec(null, row('i', h('div', { className: 'bae-m-actline bae-muted' }, 'Everything is up to date.')))));
    return h('div', { className: 'bae-m-actlist' }, out);
  }



  // ---- Android: sync activity and Downloads ----
  // MacErrorDetails on Android: the panel with a copy icon button at its corner, in a dialog.
  function AndroidErrorDetails(props) {
    return h('div', { className: 'bae-errdis-panel bae-errdis-panel-android' }, h('div', { className: 'bae-errdis-chain' }, props.details), h('span', { className: 'bae-errdis-copy' }, h(AndroidIconButton, { icon: 'copy', label: 'Copy details', size: 'small', box: 40, tone: 'muted' })));
  }
  // Android's notice: MacNotice's parts from Android's own, both buttons text buttons; centred, the primary is an outlined button. Details opens an AndroidDialog holding `AndroidErrorDetails`.
  function AndroidNotice(props) {
    var tone = props.tone || 'neutral';
    var centered = props.layout === 'centered';
    var primary = props.action ? h(AndroidButton, { key: 'p', kind: centered ? 'tonal' : 'text' }, props.action) : null;
    var secondary = props.secondary ? h(AndroidButton, { key: 's', kind: 'text' }, props.secondary) : null;
    var dismiss = props.dismiss ? h('span', { key: 'x', className: 'bae-a-notice-dismiss' }, h(AndroidInlineIconButton, { icon: 'close', label: 'Dismiss' })) : null;
    if (centered) {
      // The glyph sits beside the title only; a subtitle is its own smaller, grey line under them, so the two lines read as a heading and what to do.
      var title = h(React.Fragment, null,
        h('div', { className: 'bae-notice-centered-title' }, noticeGlyph(tone, props.icon, { centered: true }), h('div', { className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.title), props.children),
        props.subtitle ? h('div', { className: 'bae-notice-centered-sub' }, props.subtitle) : null);
      return NoticeBox(tone, props.icon, h('div', { className: 'bae-a-notice' }, title, primary || secondary ? h('div', { className: 'bae-notice-centered-row' }, secondary, primary) : null), { platform: 'bae-a-notice-box', quiet: props.quiet, centered: true, stage: props.stage });
    }
    // As on Mac: the buttons sit on the text's line at the right; `buttons: 'below'` puts them under the text.
    var below = props.buttons === 'below';
    var words = h('div', { key: 'w', className: 'bae-a-notice-words' }, noticeWords(props));
    var buttons = primary || secondary ? h('div', { key: 'b', className: below ? 'bae-a-notice-below' : 'bae-a-notice-buttons' }, secondary, primary) : null;
    // One top row, aligned to the title's line: the words, inline buttons, the ×; `below` buttons are a second row.
    var top = h('div', { className: 'bae-a-notice-top' }, words, below ? null : buttons, dismiss);
    var body = h('div', { className: 'bae-a-notice' }, top, below ? buttons : null);
    return NoticeBox(tone, props.icon, body, { platform: 'bae-a-notice-box', quiet: props.quiet, centered: false, stage: props.stage });
  }

  // A transfer in the Activity sheet, as MacTransferLine on Mac: `label`, `count`, a bar filled to `progress` (none: a moving bar). `pausable` ends the line in a tonal Pause, or Resume while `paused`; a phone never uploads release files, so Uploading has none. `opens`: the line opens a screen when tapped (Pinning opens the Pinning screen).
  function AndroidTransferLine(props) {
    return h('div', { className: cx('bae-a-actxfer', props.opens && 'bae-a-act-tap') },
      h('div', { className: 'bae-a-actxfer-line' }, h('span', null, props.label), h('span', { className: 'bae-a-actxfer-end' }, props.count ? h('span', { className: 'bae-muted' }, props.count) : null,
        props.pausable ? h(AndroidIconButton, { icon: props.paused ? 'play' : 'pause', size: 'medium', tonal: true, label: props.paused ? 'Resume' : 'Pause' }) : null)),
      h(AndroidProgress, { name: props.label, value: props.progress }));
  }
  // The Activity sheet (SyncActivity.kt's, grown to hold every problem): the title, Needs attention with the caller's notices (children), bae couldn't read sync's status (no button), and sync's own with Open sync settings; nothing retries in place; then the transfers (the downloads line opens Downloads), or up to date.
  function AndroidActivity(props) {
    var problems = props.children || props.error || props.attention;
    // Only the problems' rows start their words on the text edge, past the glyphs; the title, the Needs attention label and the transfers sit at the sheet's edge.
    var t = function (s, c) { return h('div', { className: c || 'bae-a-act-line' }, s); };
    // `held` as on Mac: only the pinning line has Pause or Resume.
    var xfer = function (key, label, count, progress, tap, held) {
      return h(AndroidTransferLine, { key: key, label: label, count: count, progress: progress, opens: tap, pausable: held != null, paused: held });
    };
    return h('div', { className: 'bae-a-act' },
      h('div', { className: 'bae-a-act-title' }, 'Activity'),
      // Each problem is a quiet notice, a line between rows.
      problems ? h('div', { className: 'bae-a-act-block' }, h(AndroidEyebrow, null, 'Needs attention'),
        h('div', { className: 'bae-a-actproblems' }, divided(kids(props.children).concat([
          props.error ? h(AndroidNotice, { key: 'e', quiet: true, dismiss: true, tone: 'danger', title: props.error }) : null,
          props.attention ? h(AndroidNotice, { key: 'f', quiet: true, dismiss: true, tone: 'warning', title: props.attention, action: 'Open sync settings', buttons: 'below' }) : null,
          props.attention && props.record ? h('div', { key: 'r' }, h('div', { className: 'bae-strong' }, props.record), h(AndroidNotice, { quiet: true, tone: 'neutral', title: props.reason })) : null].filter(Boolean))))) : null,
      // As on Mac: Transfers is its own labelled group; each transfer is its words, how far it has got, and a bar; the pinning line opens the Pinning screen.
      props.syncing || props.uploads || props.pins ? h(AndroidEyebrow, null, 'Transfers') : null,
      props.syncing ? xfer('s', 'Syncing changes', null, null) : null,
      props.uploads ? xfer('u', 'Uploading', props.uploadsWait || props.uploads, props.uploadProgress != null ? props.uploadProgress : 0.4) : null,
      props.pins ? xfer('p', 'Pinning', props.downloadsWait || props.pins, props.pinProgress != null ? props.pinProgress : 0.3, true, props.downloadsWait === 'Paused') : null,
      !(problems || props.syncing || props.uploads || props.pins) ? t('Everything is up to date.', 'bae-a-act-sub') : null);
  }
  function AndroidSegmentedControl(props) {
    var o = list(props.options);
    // TalkBack reads only the buttons: the group is unnamed, as Compose's segmented row is, and the label over it is hidden from TalkBack.
    var seg = h('div', { className: 'bae-a-seg', role: 'radiogroup' }, o.map(function (x) { var on = x === String(props.selected); return h('span', { key: x, role: 'radio', 'aria-checked': on, className: cx('bae-a-seg-btn', on && 'bae-a-seg-on') }, on ? h(Glyph, { name: 'check', size: 'medium' }) : null, x); }));
    // `label`: what the choice is, in the secondary text over the buttons, as a form names it. `helper`: what the chosen option means, in smaller secondary text under them.
    if (!props.label && !props.helper) return seg;
    return h('div', { className: 'bae-a-seg-field' },
      props.label ? h('div', { className: 'bae-a-seg-label', 'aria-hidden': true }, props.label) : null,
      seg,
      props.helper ? h('div', { className: 'bae-a-seg-helper' }, props.helper) : null);
  }

  // What bae keeps on this device, as Settings → Storage heads it: the total, one bar split by kind, and a legend line per kind with its size. `parts` is "Label:size:share,…" in the kinds' fixed order; each kind keeps its colour whatever its size or whichever kinds are shown: Pinned blue, Cache gold, Library orchid, From before the reset mint, anything else the neutral; in that order no two neighbours are hard to tell apart. The legend names every kind, so no colour stands alone.
  var USAGE_KIND = { 'Pinned': 1, 'Cache': 2, 'Library': 3, 'From before the reset': 4 };
  function storageParts(props) {
    return list(props.parts || 'Pinned:6.1 GB:0.72,Cache:1.8 GB:0.21,Images:420 MB:0.05,Library:180 MB:0.02').map(function (r) { var a = r.split(':'); return { label: a[0], size: a[1], share: Number(a[2]) }; });
  }
  // One kind in the storage legend: its colour's dot, its name (`label`, which picks the colour) and its size.
  function usageKey(p) {
    return function (props) {
      return h('span', { className: 'bae-usage-key bae-usage-key-' + p }, h('span', { className: 'bae-usage-dot bae-usage-seg-' + (USAGE_KIND[props.label] || 5), 'aria-hidden': true }), h('span', null, props.label), h('span', { className: 'bae-usage-size' }, props.size));
    };
  }
  var MacUsageKey = usageKey('m');
  var AndroidUsageKey = usageKey('a');
  function storageUsage(props, p) {
    var parts = storageParts(props);
    var Key = p === 'm' ? MacUsageKey : AndroidUsageKey;
    return h('div', { className: 'bae-usage bae-usage-' + p },
      h('div', { className: 'bae-usage-total' }, h('span', { className: 'bae-usage-num' }, props.total || '8.5 GB'), h('span', { className: 'bae-usage-of' }, props.of || (p === 'm' ? 'on this Mac' : 'on this phone'))),
      h('div', { className: 'bae-usage-bar', role: 'img', 'aria-label': parts.map(function (x) { return x.label + ' ' + x.size; }).join(', ') },
        parts.map(function (x, i) { return h('span', { key: i, className: 'bae-usage-seg bae-usage-seg-' + (USAGE_KIND[x.label] || 5), style: { flexGrow: x.share } }); })),
      h('div', { className: 'bae-usage-legend' }, parts.map(function (x, i) { return h(Key, { key: i, label: x.label, size: x.size }); })));
  }
  function MacStorageUsage(props) { return storageUsage(props, 'm'); }
  function AndroidStorageUsage(props) { return storageUsage(props, 'a'); }
  // The Pinning screen's bar: Back, the title, Retry and Pause or Resume.
  function AndroidPinningBar(props) {
    return h('header', { className: 'bae-a-dlbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }), h('span', { className: 'bae-a-dlbar-title' }, 'Pinning'),
      h(AndroidButton, { kind: 'text', off: !props.failed }, 'Retry'), h(AndroidButton, { kind: 'text', off: !!props.idle }, props.paused ? 'Resume' : 'Pause'));
  }
  // One entry of the Pinning screen: the release's name with Cancel (×) on its line, its files and size, then its state: Queued to pin, a bar with bytes, or Couldn’t pin with Details and Retry. A release since removed from the library is named in secondary text, so it doesn't read as a title.
  function AndroidPinningRow(props) {
    var s = props.state || 'queued';
    var state = s === 'queued' ? h('div', { className: 'bae-a-librow-sub' }, 'Queued to pin')
      : s === 'active' ? h('div', null, h('div', { className: 'bae-a-pinrow-bar' }, h(AndroidProgress, { name: 'Pinning', value: props.progress || 0.4 })), h('div', { className: 'bae-a-fine bae-muted' }, props.bytes || '120 MB of 300 MB'))
      : h(AndroidNotice, { quiet: true, tone: 'danger', title: 'Could not pin', secondary: 'Details', action: 'Retry' });
    return h('div', { className: 'bae-a-dlrow2' },
      h('div', { className: 'bae-a-dlrow2-head' },
        h('div', { className: cx('bae-a-librow-title', !props.title && 'bae-a-dlrow2-gone') }, props.title || 'Release no longer in library'),
        h(AndroidInlineIconButton, { icon: 'close', label: 'Cancel' })),
      h('div', { className: 'bae-a-librow-sub' }, props.detail),
      state);
  }
