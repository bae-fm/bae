  // ---- Logo and app icons ----

  // build-bundle.py puts the drawings from assets/Logos here, so the bundle works without its files beside it.
  var SHEEP_URI = 'data:image/svg+xml;base64,@@bae-sheep.svg@@';
  var SMALL_URI = { 16: 'data:image/svg+xml;base64,@@bae-favicon-16.svg@@', 32: 'data:image/svg+xml;base64,@@bae-favicon-32.svg@@' };
  var ICON_TILE = '#FFD6E7';
  function Sheep(props) {
    return h('img', { src: SHEEP_URI, alt: props.alt || '', width: props.size, style: { display: 'block', height: 'auto' } });
  }
  // The sheep drawn pixel by pixel for 16 and 32 pixels. `size` is what it's shown at, so a larger size magnifies its pixels.
  function SmallSheep(props) {
    var size = Number(props.size || 32);
    return h('img', { src: SMALL_URI[size <= 16 ? 16 : 32], alt: props.alt || '', width: size, height: size, style: { display: 'block', imageRendering: 'pixelated' } });
  }
  // The forms of the logo. `pair` is the sheep over the word, `sheep` the sheep alone, `word` the word alone, `small` the drawing for favicons and anything at 32 pixels or under. `size` is the sheep's width.
  function Logo(props) {
    var kind = props.kind || 'pair';
    var size = Number(props.size || 120);
    var word = h('div', { style: { fontSize: Math.max(34, Math.round(size * 0.4)), lineHeight: 1.1, fontWeight: 700, letterSpacing: '-1px' } }, 'bae');
    if (kind === 'small') return h(SmallSheep, { size: size, alt: 'bae' });
    if (kind === 'sheep') return h(Sheep, { size: size, alt: 'bae' });
    if (kind === 'word') return word;
    return h('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8 } }, h(Sheep, { size: size }), word);
  }
  // An app icon as each platform shows it. macOS, iOS and Android put the sheep on the pale pink tile; Windows and Linux show the sheep with no tile. Android's `mask` is the launcher's shape: circle, squircle or rounded.
  function AppIcon(props) {
    var platform = props.platform || 'macos';
    var size = Number(props.size || 128);
    if (platform === 'windows' || platform === 'linux') return h('div', { style: { width: size, height: size, display: 'flex', alignItems: 'center', justifyContent: 'center' } }, size <= 32 ? h(SmallSheep, { size: size, alt: 'bae' }) : h(Sheep, { size: size, alt: 'bae' }));
    var android = platform === 'android';
    var mask = props.mask || 'circle';
    var radius = !android ? '22.5%' : mask === 'circle' ? '50%' : mask === 'squircle' ? '36%' : '18%';
    return h('div', { style: { width: size, height: size, borderRadius: radius, background: ICON_TILE, display: 'flex', alignItems: 'center', justifyContent: 'center', overflow: 'hidden' } }, h(Sheep, { size: Math.round(size * (android ? 0.85 : 0.82)), alt: 'bae' }));
  }
