  // ---- Logo and app icons ----

  // build-bundle.py puts the drawings from assets/Logos here, so the bundle works without its files beside it.
  var SHEEP_URI = 'data:image/svg+xml;base64,@@bae-sheep.svg@@';
  var SMALL_URI = { 16: 'data:image/svg+xml;base64,@@bae-favicon-16.svg@@', 32: 'data:image/svg+xml;base64,@@bae-favicon-32.svg@@' };
  var WORD_URI = 'data:image/svg+xml;base64,@@bae-word.svg@@';
  var WORD_ASPECT = 1556 / 736;
  var ICON_TILE = '#FFD6E7';
  function Sheep(props) {
    return h('img', { src: SHEEP_URI, alt: props.alt || '', width: props.size, className: props.bob ? 'bae-sheep-bob' : undefined, style: { display: 'block', height: 'auto' } });
  }
  // The word, drawn in Fredoka SemiBold so it's the same on every platform. It takes the text colour around it.
  function Word(props) {
    var width = Math.round(props.width);
    var mask = 'url("' + WORD_URI + '") center / contain no-repeat';
    return h('div', { role: 'img', 'aria-label': 'bae', style: { width: width, height: Math.round(width / WORD_ASPECT), background: 'currentColor', WebkitMask: mask, mask: mask } });
  }
  // The sheep drawn pixel by pixel for 16 and 32 pixels. `size` is what it's shown at, so a larger size magnifies its pixels.
  function SmallSheep(props) {
    var size = Number(props.size || 32);
    return h('img', { src: SMALL_URI[size <= 16 ? 16 : 32], alt: props.alt || '', width: size, height: size, style: { display: 'block', imageRendering: 'pixelated' } });
  }
  // The forms of the logo. `pair` is the sheep over the word, `inline` the sheep before the word, which follows the page's direction, `sheep` the sheep alone, `word` the word alone, `small` the drawing for favicons and anything at 32 pixels or under. `size` is the sheep's width. `bob` makes the sheep and the word each float gently, out of step. `glow` puts a soft disc of light behind the sheep and a pool of light under it, and gives the word the sheep's cream and a thin edge at 45°.
  function Logo(props) {
    var kind = props.kind || 'pair';
    var size = Number(props.size || 120);
    var sheep = h(Sheep, { size: size, bob: props.bob });
    if (props.glow) sheep = h('div', { style: { position: 'relative', isolation: 'isolate' } }, h('div', { className: 'bae-logo-glow' }), h('div', { className: 'bae-logo-pool' }), sheep);
    if (kind === 'small') return h(SmallSheep, { size: size, alt: 'bae' });
    if (kind === 'sheep') return h(Sheep, { size: size, alt: 'bae' });
    if (kind === 'word') return h(Word, { width: size * 0.52 });
    if (kind === 'inline') return h('div', { style: { display: 'flex', alignItems: 'center', gap: Math.round(size * 0.12) } }, h(Sheep, { size: size }), h(Word, { width: size * 0.8 }));
    return h('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'center', gap: Math.round(size * 0.06) } }, sheep, h('div', { className: cx(props.glow && 'bae-word-lift', props.bob && 'bae-word-bob') }, h(Word, { width: size * 0.52 })));
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
