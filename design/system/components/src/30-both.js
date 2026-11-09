  // ---- Both ----

  // A stand-in QR code: the three finder squares and a fixed scatter of
  // modules. The apps draw the real code (QRCode in BaeKit, QRCodeImage on
  // Android).
  var QR_N = 25;
  var QR_FILL = (function () {
    var seed = 7, out = [];
    for (var i = 0; i < QR_N * QR_N; i++) { seed = (seed * 1103515245 + 12345) & 0x7fffffff; out.push(seed % 100 < 46); }
    return out;
  })();
  function qrModule(x, y) {
    function finder(fx, fy) {
      var dx = x - fx, dy = y - fy;
      if (dx < 0 || dy < 0 || dx > 6 || dy > 6) return null;
      return dx === 0 || dy === 0 || dx === 6 || dy === 6 || (dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4);
    }
    var f = finder(0, 0); if (f === null) f = finder(QR_N - 7, 0); if (f === null) f = finder(0, QR_N - 7);
    if (f !== null) return f;
    if ((x === 7 && (y < 8 || y > QR_N - 9)) || (y === 7 && (x < 8 || x > QR_N - 9)) || (x === QR_N - 8 && y < 8) || (y === QR_N - 8 && x < 8)) return false;
    return QR_FILL[y * QR_N + x];
  }
  function QrPicture(props) {
    var size = Number(props.size || 200), cells = [];
    for (var y = 0; y < QR_N; y++) for (var x = 0; x < QR_N; x++) if (qrModule(x, y)) cells.push(h('rect', { key: x + '-' + y, x: x, y: y, width: 1.02, height: 1.02 }));
    return h('div', { className: 'bae-scan-qr' },
      h('svg', { width: size, height: size, viewBox: '0 0 ' + QR_N + ' ' + QR_N, role: 'img', 'aria-label': 'QR code', shapeRendering: 'crispEdges' }, cells));
  }
  function ScanCode(prefix) {
    return function (props) {
      return h('div', { className: 'bae-scan ' + prefix + '-scan' },
        props.instruction ? h('div', { className: 'bae-scan-text' }, props.instruction) : null,
        h(QrPicture, { size: props.size }),
        h('div', { className: 'bae-scan-buttons' }, kids(props.children)),
        props.note ? h('div', { className: 'bae-scan-note' }, props.note) : null);
    };
  }
  var MacScanCode = ScanCode('bae-m');
  var AndroidScanCode = ScanCode('bae-a');

  function StatusChip(props) {
    return h('span', { className: cx('bae-chip', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  // The glyph each tone leads with, a filled circle with its mark (or, `outline`, a ring with the mark inside): BaeKit's `StatusTone.symbol` (SF Symbols) on macOS, Status.kt's `StatusTone.icon` on Android.
  // macOS draws stand-ins for info.circle.fill, checkmark.circle.fill and exclamationmark.circle.fill (warning and danger alike, told apart by colour).
  var STATUS_CIRCLE = 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20z';
  var STATUS_BANG = 'M10.9 6.5h2.2l-.3 8h-1.6zM12 15.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z';
  var STATUS_MARK = {
    info: 'M10.9 10h2.2v7.5h-2.2zM12 5.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z',
    success: 'M10.4 16.6 6.4 12.6l1.5-1.5 2.5 2.5 5.7-5.7 1.5 1.5z',
    warning: STATUS_BANG,
    danger: STATUS_BANG
  };
  // The outline form (info.circle, checkmark.circle, exclamationmark.circle): a ring, the mark solid inside it.
  var STATUS_RING = STATUS_CIRCLE + 'M12 3.6a8.4 8.4 0 1 1 0 16.8 8.4 8.4 0 1 1 0-16.8z';
  var STATUS_MATERIAL = { info: 'info', success: 'check_circle', warning: 'error', danger: 'error' };
  // A tone's glyph in the tone's colour; set="mac" for macOS. `outline` draws the outline form, for a quiet notice. Neutral, accent and activity have none.
  function StatusGlyph(props) {
    var tone = props.tone, size = Number(props.size || 15);
    if (!STATUS_MATERIAL[tone]) return null;
    var cls = cx('bae-status-glyph', 'bae-tone-' + tone, props.muted && 'bae-status-glyph-muted', props.className);
    return props.set === 'mac'
      ? h('svg', { className: cls, width: size, height: size, viewBox: '2 2 20 20', 'aria-hidden': true, fill: 'currentColor' },
          props.outline ? [h('path', { key: 'r', d: STATUS_RING, fillRule: 'evenodd' }), h('path', { key: 'm', d: STATUS_MARK[tone] })] : h('path', { d: STATUS_CIRCLE + STATUS_MARK[tone], fillRule: 'evenodd' }))
      : h(MSymbol, { name: STATUS_MATERIAL[tone], size: size, filled: !props.outline, className: cls, fill: 'currentColor' });
  }
  // The tinted box MacNotice and AndroidNotice draw in: its text the size of the screen it sits in (13 on a Mac, 14 on Android); `icon` (on unless false) leads with the tone's glyph at 1.19 times that, at the top left, level with the first line. Neutral has none. One glyph drawing serves both apps: the SF Symbol and Material's filled circle look alike.
  // `look.quiet` draws it with no fill, the screen's text, the glyph in its tone as an outline; `look.centered` centres everything in the space it's given (a view that failed whole), its glyph drawn by the caller in the title line (noticeGlyph); `look.stage` is for the lightbox's dark stage; `look.platform` is the app's own class (bae-m-notice-box or bae-a-notice-box).
  // The notice's glyph: an outline when quiet; centred, grey, in front of the title (it says something went wrong and nothing more).
  function noticeGlyph(tone, icon, look) {
    return icon !== false && STATUS_MATERIAL[tone] ? h('span', { className: 'bae-notice-icon' }, h(StatusGlyph, { tone: tone, set: 'mac', outline: !!(look.quiet || look.centered), muted: !!look.centered })) : null;
  }
  function NoticeBox(tone, icon, body, look) {
    var quiet = look.quiet;
    var glyph = look.centered ? null : noticeGlyph(tone, icon, look);
    return h('div', { className: cx('bae-notice', 'bae-tone-' + tone, look.platform, glyph && 'bae-notice-with-icon', quiet && 'bae-notice-quiet', look.centered && 'bae-notice-centered', look.stage && 'bae-notice-stage') }, glyph, glyph ? h('div', { className: 'bae-notice-body' }, body) : body);
  }
  // The notice's words: a title (bold only under a subtitle), the subtitle saying what to do, or the caller's own children.
  function noticeWords(props) {
    return [
      props.title != null ? h('div', { key: 't', className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.title) : null,
      props.subtitle ? h('div', { key: 's', className: 'bae-notice-subtitle' }, props.subtitle) : null,
      props.children != null ? h(React.Fragment, { key: 'c' }, props.children) : null];
  }
