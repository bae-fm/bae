(function () {
  var React = window.React;
  var h = React.createElement;

  function cx() {
    var out = [];
    for (var i = 0; i < arguments.length; i++) if (arguments[i]) out.push(arguments[i]);
    return out.join(' ');
  }

  // Drops the whitespace text a template leaves between child components.
  // Fragments a template's loops and conditions leave are opened, so a box sees each row.
  function kids(children) {
    var out = [];
    React.Children.toArray(children).forEach(function (c) {
      if (c && c.type === React.Fragment) out.push.apply(out, kids(c.props.children));
      else if (!(typeof c === 'string' && c.trim() === '')) out.push(c);
    });
    return out;
  }

  // The canvas draws each part inside a <div class="sc-host-x" style="display:contents">. A part that reads the parts inside it (their type, their props) reads the one inside that wrapper; `rewrap` puts a changed part back inside its wrapper, so the canvas can still select it.
  function isHost(c) {
    return !!(c && c.type === 'div' && c.props && c.props.className === 'sc-host-x');
  }
  function inner(c) {
    return isHost(c) ? React.Children.only(c.props.children) : c;
  }
  function rewrap(c, changed) {
    return isHost(c) ? React.cloneElement(c, null, changed) : changed;
  }

  // A hairline between children.
  function divided(list) {
    var out = [];
    list.forEach(function (c, i) {
      var p = inner(c);
      if (i > 0 && !(p && p.props && p.props.nested)) out.push(h('div', { key: 'd' + i, className: 'bae-divider', 'aria-hidden': true }));
      out.push(c);
    });
    return out;
  }

  // A right-pointing chevron; a class turns it to point down.
  function Chevron(props) {
    return h('svg', { className: props.className, width: 7, height: 12, viewBox: '0 0 7 12', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 1.6, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M1.5 1.5 5.5 6l-4 4.5' }));
  }

  // A length a component takes as a prop: pixels, or the name of a token (`radius-artwork`, `size-row-artwork`).
  function measure(v) {
    return /^(size|radius|space)-/.test(v) ? 'var(--' + v + ')' : Number(v);
  }

  // A glyph's size: an icon role of tokens.json, such as `medium`, or `androidTransport` for a role Android draws alone. macOS draws it at --icon-<role> and Android at --icon-android-<role>. A role the tokens lack throws, so no glyph falls back to a size of its own.
  var iconTokens = {};
  function iconSize(role, android) {
    var kebab = String(role).replace(/[A-Z]/g, function (c) { return '-' + c.toLowerCase(); });
    var name = android ? '--icon-android-' + kebab.replace(/^android-/, '') : '--icon-' + kebab;
    if (!(name in iconTokens) && typeof document !== 'undefined') iconTokens[name] = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    if (iconTokens[name] === '') throw new Error('tokens.json has no icon role `' + role + '` for ' + (android ? 'Android' : 'macOS'));
    return 'var(' + name + ')';
  }
  function iconBox(role, android) {
    var size = iconSize(role, android);
    return { width: size, height: size };
  }

  // Glyphs: the few the components draw. Stroke icons for macOS. `size` is an icon role.
  function CopyGlyph(props) {
    return h('svg', { style: iconBox(props.size), viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('rect', { x: 9, y: 9, width: 11, height: 11, rx: 2 }),
      h('path', { d: 'M5 15V5a2 2 0 0 1 2-2h8' }));
  }
  function CheckGlyph(props) {
    return h('svg', { style: iconBox(props.size), viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M5 12l5 5L20 7' }));
  }
  // Android's icons are Material Symbols (Rounded, weight 400), drawn from Google's font as SVG text so a parent's `fill` colours them.
  var SYMBOL = { back: 'arrow_back', edit: 'edit', chevron: 'chevron_right', expand: 'expand_more', more: 'more_vert', search: 'search', add: 'add', refresh: 'refresh', copy: 'content_copy',
    play: 'play_arrow', pause: 'pause', previous: 'skip_previous', next: 'skip_next', cast: 'cast', queue: 'list', repeat: 'repeat', nowplaying: 'volume_up', down: 'keyboard_arrow_down',
    warning: 'error', download: 'download', downloaded: 'download_done', pin: 'keep', unpin: 'keep_off', close: 'close', retry: 'refresh', remove: 'delete', drag: 'drag_handle', shuffle: 'shuffle', check: 'check', speakerbox: 'speaker', tv: 'tv' };
  // Status symbols (warning, error, info, check_circle) are never filled: every status glyph is the outline.
  var SYMBOL_FILLED = { play: 1, pause: 1, previous: 1, next: 1 };
  // SVG text alone doesn't make WebKit fetch a web font, so ask for the icon font once up front.
  if (typeof document !== 'undefined' && document.fonts && document.fonts.load) document.fonts.load("24px 'Material Symbols Rounded'", 'more_vert');
  // `size` is an icon role: the svg and the font are both its Android size, so the glyph fills the svg.
  function MSymbol(props) {
    var s = iconSize(props.size, true), filled = props.filled != null ? props.filled : SYMBOL_FILLED[props.name];
    // `label`: the symbol's name for a screen reader, as an image; without one, a screen reader skips it.
    // The text is the font's ligature name, which draws the glyph and is never read, so a labelled image holds only its label.
    var named = props.label ? { role: 'img', 'aria-label': props.label } : { 'aria-hidden': true };
    return h('svg', Object.assign({ style: { width: s, height: s }, className: props.className, fill: props.fill }, named),
      h('text', { 'aria-hidden': true, x: 0, y: '50%', dominantBaseline: 'central', className: 'bae-msym', style: filled ? { fontSize: s, fontVariationSettings: "'FILL' 1" } : { fontSize: s } }, SYMBOL[props.name] || props.name));
  }
  function Material(props) {
    return h(MSymbol, { name: props.name, size: props.size || 'large', className: props.className });
  }
