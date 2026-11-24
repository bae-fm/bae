  // ---- Both ----

  // The QR codes the apps' shots draw, a row of modules per string (1 a dark module), without the quiet zone, so a phone's camera reads the board's code as it reads the app's. All are at the low error-correction level the apps encode at.
  // The Mac's, by the text each holds, as Core Image encodes it (QRCode.swift), version 2: the pairing code of the component's own shot, then the device and invite codes Add device's and the invite's shots show.
  var MAC_QR = {
    'bae-pairing-4F7K-2M9Q': [
      '1111111000100101101111111',
      '1000001001000101001000001',
      '1011101010010010001011101',
      '1011101001001101101011101',
      '1011101001001101101011101',
      '1000001000111000001000001',
      '1111111010101010101111111',
      '0000000010010000100000000',
      '1110111110001101111000100',
      '1010000001011111111000011',
      '1111111010111010010111111',
      '1111010011101101111111000',
      '1011001000110010011000001',
      '0010100100110001111000111',
      '1010101111100100011001111',
      '0111010010010001001100011',
      '1000001110101001111111001',
      '0000000010011100100011011',
      '1111111011011011101010111',
      '1000001011001111100010001',
      '1011101010010010111110000',
      '1011101000110010110011010',
      '1011101010100101100111101',
      '1000001010010000010000010',
      '1111111011001001101000011'
    ],
    'CVP1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567': [
      '1111111001000010001111111',
      '1000001011011111101000001',
      '1011101001110111101011101',
      '1011101011010100101011101',
      '1011101000101011101011101',
      '1000001010100100001000001',
      '1111111010101010101111111',
      '0000000000001000100000000',
      '1111101111101111010101010',
      '0111000011000010110000101',
      '1010101111011110110000010',
      '0010000101110110111110101',
      '0011001011011100100011000',
      '1011110100101111010100110',
      '1000001111101111111010100',
      '1011010100101010111100110',
      '1010111100110001111111001',
      '0000000010011011100010100',
      '1111111010001011101010100',
      '1000001001010100100010100',
      '1011101010111001111110001',
      '1011101010100101000101111',
      '1011101010110001110101101',
      '1000001010101100110110000',
      '1111111010000100001100001'
    ],
    'CVI1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567': [
      '1111111000100001101111111',
      '1000001011100111001000001',
      '1011101010000110001011101',
      '1011101001001000101011101',
      '1011101011101100101011101',
      '1000001010101010001000001',
      '1111111010101010101111111',
      '0000000010110000000000000',
      '1101001100011110101110110',
      '0100110011011110101100000',
      '0010011000011001110111111',
      '1110000111111000110000101',
      '0000011000111111000000111',
      '0110100000010111101100001',
      '1001111100011110001011010',
      '0110000100110110100000101',
      '1100101110110110111110001',
      '0000000011110101100010101',
      '1111111011101000101011000',
      '1000001001101100100010011',
      '1011101001001000111111111',
      '1011101011011001011001100',
      '1011101000110110110010101',
      '1000001010100010111000001',
      '1111111010100111101111101'
    ]
  };
  // Android's, by the text each holds, as ZXing encodes it (QRCodeImage), version 3: the invite code with no S3 key that the component's own shot and the invite's shot draw, then the device code Add device's shot shows.
  var ANDROID_QR = {
    'CVI1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567': [
      '11111110001011110110001111111',
      '10000010101001111011001000001',
      '10111010000011101110101011101',
      '10111010111011110110101011101',
      '10111010010000011000001011101',
      '10000010110111110010101000001',
      '11111110101010101010101111111',
      '00000000011101010101100000000',
      '11111011110110101001110101010',
      '01100100101011101111001111110',
      '01011011101010110101111011101',
      '00101100100010111011110101101',
      '10001011111101001000100100000',
      '00101000100110100001110001100',
      '10001110000011101010000111100',
      '01111000101101111001001111111',
      '10011011110111011000110101000',
      '10111001110000100111000000101',
      '10111010111101001101101110111',
      '10010000000010011101101101001',
      '10111110000001101000111111010',
      '00000000110110100101100011011',
      '11111110111111110111101011111',
      '10000010001011111010100011011',
      '10111010111111000000111111010',
      '10111010101011101001001101011',
      '10111010111111110101111010110',
      '10000010111000101001000011011',
      '11111110111000011001111100110'
    ],
    'CVP1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567': [
      '11111110001011110110001111111',
      '10000010101001111011001000001',
      '10111010000011101110101011101',
      '10111010111011110110101011101',
      '10111010010000011000001011101',
      '10000010110111110010101000001',
      '11111110101010101010101111111',
      '00000000011101010101100000000',
      '11111011110110101001110101010',
      '01000000101011101111001111110',
      '00111011001010110101111011101',
      '11111100100010111011110101100',
      '11101111011101001000100100001',
      '11111101010110100001110001110',
      '10100010011011101010000111101',
      '00100101111101111001001111110',
      '11000110110111011000110101011',
      '11001101000000100111000000101',
      '10110011101101001101101110111',
      '10010001011010011101101101001',
      '10101011001001101000111111010',
      '00000000111110100101100011011',
      '11111110110111110111101011111',
      '10000010010011111010100011011',
      '10111010101111000000111111010',
      '10111010101011101001001101011',
      '10111010110111110101111010110',
      '10000010101000101001000011011',
      '11111110101000011001111100110'
    ]
  };
  // Each module is 360/105 pt, the side a phone's camera was measured to need, inside the standard's quiet zone of four modules, and the whole square, quiet zone and all, is the QR code.
  var QR_MODULE = 360 / 105, QR_QUIET = 4;
  function QrPicture(props) {
    var rows = props.rows, n = rows.length, cells = [];
    for (var y = 0; y < n; y++) for (var x = 0; x < n; x++) if (rows[y].charAt(x) === '1') cells.push(h('rect', { key: x + '-' + y, x: x, y: y, width: 1.02, height: 1.02 }));
    if (props.module) {
      var m = props.module;
      return h('div', { className: 'bae-scan-qr bae-scan-qr-modules', role: 'img', 'aria-label': 'QR code', style: { padding: QR_QUIET * m } },
        h('svg', { width: n * m, height: n * m, viewBox: '0 0 ' + n + ' ' + n, 'aria-hidden': true, shapeRendering: 'crispEdges' }, cells));
    }
    var size = Number(props.size || 200);
    return h('div', { className: 'bae-scan-qr' },
      h('svg', { width: size, height: size, viewBox: '0 0 ' + n + ' ' + n, role: 'img', 'aria-label': 'QR code', shapeRendering: 'crispEdges' }, cells));
  }
  // Without `size`, a code is as wide as its modules need, as the apps draw it. `code` is the text the QR code holds, one of MAC_QR's on the Mac and one of ANDROID_QR's on Android. On Android, `size` is the side of the whole code, quiet zone included, as QRCodeImage draws it; on the Mac, it sets the picture's side on a printed page.
  function ScanCode(prefix) {
    return function (props) {
      var rows = prefix === 'bae-a' ? ANDROID_QR[props.code || 'CVI1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'] : MAC_QR[props.code || 'bae-pairing-4F7K-2M9Q'];
      var qr = props.size == null ? { rows: rows, module: QR_MODULE }
        : prefix === 'bae-a' ? { rows: rows, module: Number(props.size) / (rows.length + 2 * QR_QUIET) }
        : { rows: rows, size: props.size };
      return h('div', { className: 'bae-scan ' + prefix + '-scan' },
        props.instruction ? h('div', { className: 'bae-scan-text' }, props.instruction) : null,
        h(QrPicture, qr),
        h('div', { className: 'bae-scan-buttons' }, kids(props.children)));
    };
  }
  // The six digits two devices both show while one lets the other in, so the person can check they match: large, spaced in two groups of three, centred. `waiting` says what the device is waiting for, beside a spinner so the screen shows it's still checking, ending in an ellipsis. `dialog` sets it a step further from the line above it, inside a dialog or sheet; on Android it's also the smaller size.
  function MatchCode(prefix, Spin) {
    return function (props) {
      return h('div', { className: cx(prefix + '-code', props.dialog && prefix + '-code-dialog') },
        h('div', { className: prefix + '-code-digits' }, props.code),
        props.waiting ? h(Spin, { size: 16, label: props.waiting }) : null);
    };
  }
  var MacMatchCode = MatchCode('bae-m', MacSpinner);
  var AndroidMatchCode = MatchCode('bae-a', AndroidSpinner);

  var MacScanCode = ScanCode('bae-m');
  var AndroidScanCode = ScanCode('bae-a');

  // A bar's track and fill. `value` is 0 to 1, or null for a wait of no known length, a moving bar; `android` draws Material's bar, with a gap before its end dot. `label` names it for a screen reader (what it measures: "Pinning", "Finding libraries"): VoiceOver and TalkBack read a bar by its name, so every bar has one. `className` is for the caller's layout.
  function ProgressBar(props) {
    var known = props.value != null;
    return h('div', { className: cx('bae-progress-track', props.android && 'bae-progress-android', props.className), role: 'progressbar', 'aria-label': props.label, 'aria-valuenow': known ? Math.round(props.value * 100) : undefined },
      h('div', { className: cx('bae-progress-fill', !known && 'bae-progress-moving'), style: known ? { width: (props.value * 100) + '%' } : null }));
  }
  function StatusChip(props) {
    return h('span', { className: cx('bae-chip', props.android && 'bae-a-chip', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  // The glyph each tone leads with, a ring with its mark inside, the one form every status glyph takes: BaeKit's `StatusTone.symbol` (SF Symbols) on macOS, Status.kt's `StatusTone.icon` on Android.
  // Two drawings, each across a 20-unit box: `mac`, the stand-ins for info.circle, checkmark.circle and exclamationmark.circle (warning and danger alike, told apart by colour), measured from the app's shots (the ring 1.017 times the icon size across, its stroke 0.112 times it); and `ring`, the ring with its solid mark that Android's notices lead with (Status.kt's StatusRingIcons).
  // Each mark is a filled shape (`fill`) or a line drawn 2 units thick with round ends (`stroke`).
  var MAC_BANG = { fill: 'M10.85 8.45a1.15 1.15 0 0 1 2.3 0L13 12.75a1 1 0 0 1-2 0zM12 14.6a1.25 1.25 0 1 0 0 2.5 1.25 1.25 0 1 0 0-2.5z' };
  var RING_BANG = { fill: 'M10.9 6.5h2.2l-.3 8h-1.6zM12 15.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z' };
  var STATUS_DRAWING = {
    mac: {
      ring: 'M12 1.83a10.17 10.17 0 1 0 0 20.34 10.17 10.17 0 1 0 0-20.34zM12 4.07a7.93 7.93 0 1 1 0 15.86 7.93 7.93 0 1 1 0-15.86z',
      info: { fill: 'M11.75 6.35a1.35 1.35 0 1 0 0 2.7 1.35 1.35 0 1 0 0-2.7zM9 10.6h4.25v5.45h2.15v1.45H9v-1.45h2.15v-4.15H9z' },
      success: { stroke: 'M7.4 12.6 9.4 15.6 14.7 8.6' },
      warning: MAC_BANG,
      danger: MAC_BANG
    },
    ring: {
      ring: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20zM12 3.6a8.4 8.4 0 1 1 0 16.8 8.4 8.4 0 1 1 0-16.8z',
      info: { fill: 'M10.9 10h2.2v7.5h-2.2zM12 5.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z' },
      success: { fill: 'M10.4 16.6 6.4 12.6l1.5-1.5 2.5 2.5 5.7-5.7 1.5 1.5z' },
      warning: RING_BANG,
      danger: RING_BANG
    }
  };
  function statusMark(m) {
    return m.stroke ? h('path', { d: m.stroke, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' }) : h('path', { d: m.fill });
  }
  var STATUS_MATERIAL = { info: 'info', success: 'check_circle', warning: 'error', danger: 'error' };
  // A tone's glyph in the tone's colour; set="mac" for macOS, set="ring" for the ring an Android notice leads with, none for Android's Material symbol. `size` is an icon role; `inline` (macOS) sizes it at 1.19 times the text beside it instead; `muted` draws it in secondary text. Neutral, accent and activity have none.
  function StatusGlyph(props) {
    var tone = props.tone;
    if (!STATUS_MATERIAL[tone]) return null;
    var cls = cx('bae-status-glyph', 'bae-tone-' + tone, props.muted && 'bae-status-glyph-muted', props.inline && 'bae-status-glyph-inline', props.className);
    var drawing = STATUS_DRAWING[props.set];
    return drawing
      ? h('svg', { className: cls, style: props.inline ? null : iconBox(props.size), viewBox: '2 2 20 20', 'aria-hidden': true, fill: 'currentColor' },
          h('path', { d: drawing.ring, fillRule: 'evenodd' }), statusMark(drawing[tone]))
      : h(MSymbol, { name: STATUS_MATERIAL[tone], size: props.size, className: cls, fill: 'currentColor' });
  }
  // The tinted box MacNotice and AndroidNotice draw in: its text the size of the screen it sits in (13 on a Mac, 14 on Android); `icon` (on unless false) leads with the tone's glyph at 1.19 times that, at the top left, level with the first line. Neutral has none. A Mac notice draws the SF Symbol's stand-in, an Android notice its ring.
  // `look.quiet` draws it with no fill, the screen's text; `look.centered` centres everything in the space it's given (a view that failed whole), its glyph drawn by the caller in the title line (noticeGlyph); `look.stage` is for the lightbox's dark stage; `look.platform` is the app's own class (bae-m-notice-box or bae-a-notice-box). `look.android` leads with Android's ring (`set: 'ring'`) in place of the macOS glyph.
  // The notice's glyph; centred, grey, in front of the title (it says something went wrong and nothing more).
  function noticeGlyph(tone, icon, look) {
    return icon !== false && STATUS_MATERIAL[tone] ? h('span', { className: 'bae-notice-icon' }, h(StatusGlyph, { tone: tone, set: look.android ? 'ring' : 'mac', inline: true, muted: !!look.centered })) : null;
  }
  function NoticeBox(tone, icon, body, look) {
    var quiet = look.quiet;
    var glyph = look.centered ? null : noticeGlyph(tone, icon, look);
    return h('div', { className: cx('bae-notice', 'bae-tone-' + tone, look.platform, glyph && 'bae-notice-with-icon', quiet && 'bae-notice-quiet', look.centered && 'bae-notice-centered', look.stage && 'bae-notice-stage') }, glyph, glyph ? h('div', { className: 'bae-notice-body' }, body) : body);
  }
  // The notice's words: a title (bold only under a subtitle), the subtitle saying what to do, or the caller's own children.
  function noticeWords(props) {
    return [
      props.title != null ? h('div', { key: 't', className: cx('bae-notice-title', props.subtitle && 'bae-notice-title-strong') }, props.opens ? h('span', { role: 'button' }, props.title) : props.title) : null,
      props.subtitle ? h('div', { key: 's', className: 'bae-notice-subtitle' }, props.subtitle) : null,
      props.children != null ? h(React.Fragment, { key: 'c' }, props.children) : null];
  }
