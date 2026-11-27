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
  // A glyph's size in points: a number of points, or a macOS icon role's token.
  function iconPoints(size) {
    if (typeof size === 'number') return size;
    var token = iconSize(size).slice(4, -1);
    return parseFloat(iconTokens[token]);
  }
  // A length token's value in points (`type-body-size`, `size-lead`). A token the tokens lack throws.
  function tokenPoints(name) {
    var points = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--' + name));
    if (isNaN(points)) throw new Error('tokens.json has no length `' + name + '`');
    return points;
  }
  function iconBox(role, android) {
    var size = iconSize(role, android);
    return { width: size, height: size };
  }

  // The components' names for Android's icons, each a Material Symbol's name.
  var SYMBOL = { back: 'arrow_back', edit: 'edit', chevron: 'chevron_right', expand: 'expand_more', more: 'more_vert', search: 'search', add: 'add', refresh: 'refresh', copy: 'content_copy',
    play: 'play_arrow', pause: 'pause', previous: 'skip_previous', next: 'skip_next', cast: 'cast', queue: 'list', repeat: 'repeat', nowplaying: 'volume_up', down: 'keyboard_arrow_down',
    warning: 'error', download: 'download', downloaded: 'download_done', pin: 'keep', unpin: 'keep_off', close: 'close', retry: 'refresh', remove: 'delete', drag: 'drag_handle', shuffle: 'shuffle', check: 'check', speakerbox: 'speaker', tv: 'tv' };
  // Android's icons are Material Symbols Rounded at weight 400 and optical size 24: each is Google's drawing in its box of 960 units, which runs from -960 to 0 vertically, as the app's SymbolIcons draws it. Play, pause, previous, next and favorite are the filled forms; every other symbol, the status ones among them, is the outline.
  // build-bundle.py puts each drawing of assets/Symbols here by its file's name, as its viewBox and its paths, so the components and the website draw from the same files.
  var SYMBOL_DRAWING = /* assets/Symbols */ {};
  // A drawing's paths, as children of its svg.
  function symbolPaths(drawing) {
    return drawing.paths.map(function (path, i) { return h('path', Object.assign({ key: i }, path)); });
  }
  // `size` is an icon role: the svg is its Android size, and the symbol's box fills the svg, as the app sizes its icon; or a number of points, where the app's component takes its size from its caller. `fill` colours it.
  function MSymbol(props) {
    var name = SYMBOL[props.name] || props.name;
    var drawing = SYMBOL_DRAWING[name];
    if (!drawing) throw new Error('assets/Symbols has no drawing of the Material Symbol `' + name + '`');
    var s = typeof props.size === 'number' ? props.size : iconSize(props.size, true);
    // `label`: the symbol's name for a screen reader, as an image; without one, a screen reader skips it.
    var named = props.label ? { role: 'img', 'aria-label': props.label } : { 'aria-hidden': true };
    return h('svg', Object.assign({ viewBox: drawing.viewBox, style: { width: s, height: s }, className: props.className, fill: props.fill }, named),
      symbolPaths(drawing));
  }
  function Material(props) {
    return h(MSymbol, { name: props.name, size: props.size || 'large', className: props.className });
  }
