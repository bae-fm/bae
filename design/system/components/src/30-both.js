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

  function Notice(props) {
    return h('div', { className: cx('bae-notice', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }
