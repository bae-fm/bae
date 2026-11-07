  // ---- Logo and app icons ----

  // build-bundle.py puts assets/Logos/bae-sheep.svg here, so the bundle works without its files beside it.
  var SHEEP_URI = 'data:image/svg+xml;base64,@@bae-sheep.svg@@';
  var ICON_TILE = '#FFD6E7';
  function Sheep(props) {
    return h('img', { src: SHEEP_URI, alt: props.alt || '', width: props.size, style: { display: 'block', height: 'auto' } });
  }
  // The three forms of the logo. `pair` is the sheep over the word, `sheep` the sheep alone, `word` the word alone. `size` is the sheep's width.
  function Logo(props) {
    var kind = props.kind || 'pair';
    var size = Number(props.size || 120);
    var word = h('div', { style: { fontSize: Math.max(34, Math.round(size * 0.4)), lineHeight: 1.1, fontWeight: 700, letterSpacing: '-1px' } }, 'bae');
    if (kind === 'sheep') return h(Sheep, { size: size, alt: 'bae' });
    if (kind === 'word') return word;
    return h('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8 } }, h(Sheep, { size: size }), word);
  }
  // An app icon as each platform shows it. macOS, iOS and Android put the sheep on the pale pink tile; Windows and Linux show the sheep with no tile. Android's `mask` is the launcher's shape: circle, squircle or rounded.
  function AppIcon(props) {
    var platform = props.platform || 'macos';
    var size = Number(props.size || 128);
    if (platform === 'windows' || platform === 'linux') return h('div', { style: { width: size, height: size, display: 'flex', alignItems: 'center', justifyContent: 'center' } }, h(Sheep, { size: size, alt: 'bae' }));
    var android = platform === 'android';
    var mask = props.mask || 'circle';
    var radius = !android ? '22.5%' : mask === 'circle' ? '50%' : mask === 'squircle' ? '36%' : '18%';
    return h('div', { style: { width: size, height: size, borderRadius: radius, background: ICON_TILE, display: 'flex', alignItems: 'center', justifyContent: 'center', overflow: 'hidden' } }, h(Sheep, { size: Math.round(size * (android ? 0.85 : 0.82)), alt: 'bae' }));
  }
