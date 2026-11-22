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

  // Glyphs: the few the components draw. Stroke icons for macOS, Material paths for Android. `size` is an icon role.
  function CopyGlyph(props) {
    return h('svg', { style: iconBox(props.size), viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('rect', { x: 9, y: 9, width: 11, height: 11, rx: 2 }),
      h('path', { d: 'M5 15V5a2 2 0 0 1 2-2h8' }));
  }
  function CheckGlyph(props) {
    return h('svg', { style: iconBox(props.size), viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M5 12l5 5L20 7' }));
  }
  var MATERIAL = {
    back: 'M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z',
    edit: 'M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 000-1.41l-2.34-2.34a1 1 0 00-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z',
    chevron: 'M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
    expand: 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z',
    more: 'M12 8a2 2 0 100-4 2 2 0 000 4zm0 2a2 2 0 100 4 2 2 0 000-4zm0 6a2 2 0 100 4 2 2 0 000-4z',
    search: 'M15.5 14h-.79l-.28-.27A6.47 6.47 0 0016 9.5 6.5 6.5 0 109.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
    add: 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z',
    refresh: 'M17.65 6.35A7.95 7.95 0 0012 4a8 8 0 108 8h-2a6 6 0 11-1.76-4.24L13 11h7V4l-2.35 2.35z',
    copy: 'M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z'
  };
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
    return h('svg', { style: { width: s, height: s }, 'aria-hidden': true, className: props.className, fill: props.fill },
      h('text', { x: 0, y: '50%', dominantBaseline: 'central', className: 'bae-msym', style: filled ? { fontSize: s, fontVariationSettings: "'FILL' 1" } : { fontSize: s } }, SYMBOL[props.name] || props.name));
  }
  function Material(props) {
    return h(MSymbol, { name: props.name, size: props.size || 'large', className: props.className });
  }
