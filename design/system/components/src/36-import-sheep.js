  // ---- The sheep taking in releases ----

  // The sheep's drawing split into the parts that move: read once from the same file the logo shows, so the two can't drift apart.
  var sheepParts = null;
  function readSheepParts() {
    if (sheepParts) return sheepParts;
    var doc = new DOMParser().parseFromString(atob(SHEEP_URI.slice(SHEEP_URI.indexOf(',') + 1)), 'image/svg+xml');
    var out = new XMLSerializer();
    function whole(id) { return out.serializeToString(doc.getElementById(id)); }
    function some(id, from, to) { return Array.prototype.slice.call(doc.getElementById(id).children, from, to).map(function (el) { return out.serializeToString(el); }).join(''); }
    sheepParts = {
      defs: Array.prototype.map.call(doc.querySelector('defs').children, function (el) { return out.serializeToString(el); }).join(''),
      headband: whole('headband'), earcupL: whole('left-earcup'), body: whole('body'),
      // The ears' group holds the left ear's three layers, then the right's.
      earL: some('ears', 0, 3), earR: some('ears', 3, 6),
      // The face: its fill, wool and outline; the two cheeks; the mouth and tongue; the closed eyes.
      face: some('face', 0, 3), cheekL: some('face', 3, 4), cheekR: some('face', 4, 5), mouth: some('face', 5, 7), eyes: some('face', 7, 8),
      wool: whole('head'), earcupR: whole('right-earcup'),
      // The player: its case and screen (and the screen alone), the heart on the screen, the screen's bubbles.
      player: some('player', 0, 4), screen: some('player', 3, 4), heart: some('player', 4, 5), bubbles: some('player', 5, 6),
      handL: some('hands', 0, 3), handR: some('hands', 3, 6)
    };
    return sheepParts;
  }
  var importSheepCount = 0;
  // The drawing's ids, made unique to one sheep, so two on a page don't share clip paths.
  function ownIds(markup, prefix) {
    return markup.replace(/id="([\w-]+)"/g, 'id="' + prefix + '$1"').replace(/url\(#([\w-]+)\)/g, 'url(#' + prefix + '$1)').replace(/href="#([\w-]+)"/g, 'href="#' + prefix + '$1"');
  }

  var PLUM = '#40183d';
  var CREAM = '#fffcf4';
  // A made-up release, drawn to fit a circle of radius 50 around 0,0, in the sheep's line: a cover, a vinyl record, a CD, or a record half out of its sleeve.
  function Release(props) {
    var tone = 'var(--accent-' + (props.tone || 'blue') + '-fill)';
    var line = { stroke: PLUM, strokeWidth: 9 };
    var disc = 'M0-50a50 50 0 1 1 0 100a50 50 0 1 1 0-100ZM0-5a5 5 0 1 0 0 10a5 5 0 1 0 0-10Z';
    if (props.kind === 'vinyl') return h('g', null,
      h('path', Object.assign({ d: disc, fillRule: 'evenodd', style: { fill: tone } }, line)),
      h('g', { fill: 'none', stroke: PLUM, strokeOpacity: 0.3, strokeWidth: 3 }, h('circle', { r: 40 }), h('circle', { r: 31 })),
      h('path', { d: 'M-30-26a40 40 0 0 1 22-12', fill: 'none', stroke: '#ffffff', strokeOpacity: 0.6, strokeWidth: 6 }),
      h('path', { d: 'M0-19a19 19 0 1 1 0 38a19 19 0 1 1 0-38ZM0-5a5 5 0 1 0 0 10a5 5 0 1 0 0-10Z', fill: CREAM, fillRule: 'evenodd', stroke: PLUM, strokeWidth: 4 }));
    if (props.kind === 'cd') return h('g', null,
      h('path', Object.assign({ d: 'M0-50a50 50 0 1 1 0 100a50 50 0 1 1 0-100ZM0-10a10 10 0 1 0 0 20a10 10 0 1 0 0-20Z', fill: '#e9e5f4', fillRule: 'evenodd' }, line)),
      h('path', { d: 'M-34-22a40 40 0 0 1 30-18', fill: 'none', strokeWidth: 9, style: { stroke: 'var(--accent-sky-fill)' } }),
      h('path', { d: 'M34 22a40 40 0 0 1-30 18', fill: 'none', strokeWidth: 9, style: { stroke: tone } }),
      h('circle', { r: 19, fill: 'none', stroke: PLUM, strokeOpacity: 0.35, strokeWidth: 4 }));
    var cover = h('g', null,
      h('rect', Object.assign({ x: -50, y: -50, width: 100, height: 100, rx: 10, style: { fill: tone } }, line)),
      h('circle', { cx: 14, cy: -12, r: 17, fill: '#ffffff', fillOpacity: 0.5 }),
      h('path', { d: 'M-45 26C-22 10 6 34 45 16', fill: 'none', stroke: PLUM, strokeOpacity: 0.3, strokeWidth: 7 }));
    if (props.kind !== 'sleeve') return cover;
    return h('g', null, h('g', { transform: 'translate(24 0) scale(.9)' }, h(Release, { kind: 'vinyl', tone: props.disc || 'pink' })), h('g', { transform: 'translate(-12 0)' }, cover));
  }

  // One release in one moment of the sequence. `id` names its two tracks, `item-<id>` (where it is) and `spin-<id>` (how it sways or turns); `kind` and `tone` are its drawing; `size` its radius.
  function Incoming(props) {
    return h('g', { className: 'bae-isheep-item-' + props.id },
      h('g', { className: 'bae-isheep-spin-' + props.id }, h('g', { transform: 'scale(' + (props.size / 50) + ')' }, h(Release, { kind: props.kind, tone: props.tone, disc: props.disc }))));
  }

  // ---- How the sheep moves ----
  // The sheep plays one moment at a time from a fixed sequence, each moment followed by a rest. A moment is `dur` seconds long, rest
  // included. Each of its tracks moves one part of the drawing (a `bae-isheep-<part>` group) through keys: [seconds, properties,
  // easing of the move to the next key]. A string stands for a transform. Every track starts and ends at rest, so the moments join
  // end to end into one loop, which becomes CSS keyframes once, in a style sheet of their own. bundle.css holds what doesn't move:
  // each part's pivot, what starts hidden, and the still pose.
  var ISHEEP_EASE = { io: 'cubic-bezier(.45,0,.55,1)', o: 'cubic-bezier(.2,.7,.3,1)', i: 'cubic-bezier(.55,0,.85,.35)', l: 'linear', back: 'cubic-bezier(.3,1.5,.55,1)', snap: 'cubic-bezier(.8,0,1,.6)', hold: 'steps(1, end)' };
  // A transform in the drawing's units: moved by x, y, turned by r degrees, scaled by sx, sy.
  function at(x, y, r, sx, sy) {
    sx = sx === undefined ? 1 : sx;
    return 'translate(' + (x || 0) + 'px, ' + (y || 0) + 'px) rotate(' + (r || 0) + 'deg) scale(' + sx + ', ' + (sy === undefined ? sx : sy) + ')';
  }
  var REST = at();
  // Keys along a path: `fn(u)` gives [x, y, r, scale] for u from 0 to 1, `more(u)` any other properties.
  function along(fn, t0, t1, step, more, ease) {
    var n = Math.max(2, Math.round((t1 - t0) / step)), out = [];
    for (var i = 0; i <= n; i++) {
      var u = i / n, p = fn(u), props = { transform: at(p[0], p[1], p[2], p[3]) };
      if (more) Object.assign(props, more(u));
      out.push([t0 + (t1 - t0) * u, props, ease || 'l']);
    }
    return out;
  }
  // Ears turn the other way from each other: the left ear rises with a positive angle, the right with a negative one.
  var EARS = [['earL', 1], ['earR', -1]];
  // The face turned toward a side: the features slide that way, and the far cheek narrows while the near one widens.
  function faceToward(t, d, on, off, shift) {
    t.features = [[0, REST], [on[0], REST], [on[1], at(shift * d, 0)], [off[0], at(shift * d, 0)], [off[1], REST]];
  }

  // gobble: a line of three releases slides in from one side (`d` 1 from the right, -1 from the left) on a steady beat. The sheep
  // turns its face to them, opens wide and chomps each as it arrives, munches with full cheeks, swallows, and the heart on its player pulses.
  function gobbleMoment(id, d, releases) {
    var dur = 7, t = {}, beat = 0.36, arrive = [1.8, 1.8 + beat, 1.8 + 2 * beat], last = arrive[2], y = 300, speed = 130 / beat;
    // Where the releases enter the mouth, mirrored about the mouth's middle (x 226) for the left.
    function x(right) { return d > 0 ? right : 452 - right; }
    var x0 = d > 0 ? 820 : -236;
    arrive.forEach(function (ta, i) {
      var start = ta - 0.06 - Math.abs(x0 - x(306)) / speed, far = at(x0, y), gone = at(x(276), y + 6, 0, 0.45), name = id + '-' + (i + 1);
      t['item-' + name] = [[0, { transform: far, opacity: 0 }, 'l'], [start, { transform: far, opacity: 0 }, 'l'],
        [start + 0.15, { transform: at(x0 - d * speed * 0.15, y), opacity: 1 }, 'l'], [ta - 0.06, { transform: at(x(306), y), opacity: 1 }, 'l'],
        [ta - 0.01, { transform: at(x(284), y + 4, 0, 0.62), opacity: 1 }, 'l'], [ta + 0.02, { transform: gone, opacity: 0 }, 'l'], [dur, { transform: gone, opacity: 0 }, 'l']];
      // Each release sways in step with the beat on its way in.
      var sway = [[0, REST, 'io']];
      for (var k = 0, s = start; s < ta; k++, s += beat / 2) sway.push([s, at(0, k % 2 ? -6 : 0, k % 2 ? -7 : 7), 'io']);
      sway.push([ta + 0.1, REST, 'io']);
      t['spin-' + name] = sway;
    });
    faceToward(t, d, [0.9, 1.3], [4.3, 4.75], 34);
    var head = [[0, REST], [0.9, REST], [1.3, at(6 * d, 0, 3 * d)], [1.42, at(6 * d, 0, 3 * d)], [1.66, at(-2 * d, -10, -2 * d), 'snap']];
    arrive.forEach(function (ta) { head.push([ta, at(14 * d, 6, 4 * d), 'o'], [ta + 0.22, at(4 * d, -5, 0), 'snap']); });
    head.pop();
    t.head = head.concat([[2.85, at(6 * d, 0, 2 * d)], [3.05, at(6 * d, 5, 2 * d)], [3.3, at(6 * d, -2, d)], [3.55, at(6 * d, 5, 2 * d)], [3.8, at(6 * d, -5, d), 'i'], [3.98, at(4 * d, 14, d), 'o'], [4.3, at(3 * d, -6, d)], [4.75, REST]]);
    t.mouth = [[0, { opacity: 1 }], [1.42, { opacity: 1 }, 'l'], [1.5, { opacity: 0 }], [4.24, { opacity: 0 }, 'l'], [4.32, { opacity: 1 }]];
    var shut = at(0, -2, 0, 0.85, 0.1);
    var gape = [[0, { opacity: 0, transform: shut }], [1.42, { opacity: 0, transform: shut }, 'l'], [1.48, { opacity: 1, transform: shut }, 'o'], [1.68, { opacity: 1, transform: REST }, 'snap']];
    arrive.forEach(function (ta) {
      gape.push([ta + 0.05, { opacity: 1, transform: shut }, 'o']);
      if (ta !== last) gape.push([ta + 0.25, { opacity: 1, transform: REST }, 'snap']);
    });
    t.gape = gape.concat([[last + 0.06, { opacity: 1, transform: shut }, 'l'], [last + 0.07, { opacity: 0, transform: shut }]]);
    t.munch = munching(last + 0.07, [3.05, 3.55], 4.22);
    var near = d > 0 ? 'cheekR' : 'cheekL', far2 = d > 0 ? 'cheekL' : 'cheekR';
    [[far2, 1.1], [near, 0.8]].forEach(function (c) {
      var sx = c[1];
      t[c[0]] = [[0, REST], [0.9, REST], [1.3, at(0, 0, 0, sx, 1)], [last, at(0, 0, 0, sx, 1), 'back'], [2.75, at(0, 0, 0, sx * 1.45, 1.45)], [3.85, at(0, 0, 0, sx * 1.4, 1.4), 'o'], [4.1, at(0, 0, 0, sx, 1)], [4.3, at(0, 0, 0, sx, 1)], [4.75, REST]];
    });
    t.eyes = [[0, REST], [2.6, REST], [2.8, at(0, 3, 0, 1.06, 0.72)], [3.9, at(0, 3, 0, 1.06, 0.72), 'o'], [4.2, REST]];
    var rig = [[0, REST], [1.42, REST], [1.66, at(0, 0, 0, 0.98, 1.03), 'snap']];
    arrive.forEach(function (ta) { rig.push([ta, at(0, 0, 0, 1.03, 0.97), 'o'], [ta + 0.22, at(0, 0, 0, 0.995, 1.005), 'snap']); });
    rig.pop();
    t.rig = rig.concat(swallow(3.8));
    EARS.forEach(function (e) {
      var s = e[1], keys = [[0, REST], [0.9, REST], [1.3, at(0, 0, 8 * s), 'o']];
      arrive.forEach(function (ta) { keys.push([ta + 0.06, at(0, 0, -6 * s), 'o'], [ta + 0.28, at(0, 0, 6 * s)]); });
      t[e[0]] = keys.concat([[3.0, at(0, 0, 4 * s)], [4.05, at(0, 0, -8 * s), 'o'], [4.35, at(0, 0, 22 * s)], [4.65, at(0, 0, -6 * s)], [4.95, at(0, 0, 2 * s)], [5.25, REST]]);
    });
    t.heart = [[0, REST], [4.05, REST, 'back'], [4.3, at(0, 0, 0, 1.5)], [4.8, REST]];
    return { dur: dur, tracks: t, releases: releases };
  }
  // The closed, munching mouth: it shows at `from`, chews at each of `chews`, and gives way to the usual mouth at `to`.
  function munching(from, chews, to) {
    var chew = at(0, 4, 0, 1.15, 0.45), keys = [[0, { opacity: 0, transform: REST }], [from - 0.03, { opacity: 0, transform: REST }, 'l'], [from, { opacity: 1, transform: REST }]];
    chews.forEach(function (c) { keys.push([c, { opacity: 1, transform: chew }], [c + 0.25, { opacity: 1, transform: REST }]); });
    return keys.concat([[to, { opacity: 1, transform: REST }, 'l'], [to + 0.08, { opacity: 0, transform: REST }]]);
  }
  // A swallow starting at `t0`: the body squashes, stretches and settles.
  function swallow(t0) {
    return [[t0 - 0.6, REST], [t0, REST, 'i'], [t0 + 0.2, at(0, 0, 0, 1.07, 0.92), 'o'], [t0 + 0.45, at(0, 0, 0, 0.97, 1.04)], [t0 + 0.7, at(0, 0, 0, 1.01, 0.99)], [t0 + 0.95, REST]];
  }

  // catch: one release spins down from above, from one side (`from` 'right' or 'left') or straight above ('top'). The sheep startles
  // and looks toward it, crouches, leaps, catches it into its player, lands with a squash and a burst of stars, blushes, and a heart floats up.
  function catchMoment(id, from, release) {
    var dur = 7.5, t = {}, lift = -120, caught = 1.95, land = 2.32, to = [222, 282], name = id + '-1';
    var d = from === 'right' ? 1 : from === 'left' ? -1 : 0;
    var path = {
      right: { start: [700, -220], bend: [640, 330] },
      left: { start: [-160, -240], bend: [-200, 330] },
      top: { start: [222, -300], bend: [222, 0] }
    }[from];
    function fall(u) {
      var w = Math.pow(u, d ? 1.25 : 1.6), a = (1 - w) * (1 - w), b = 2 * (1 - w) * w, c = w * w;
      return [a * path.start[0] + b * path.bend[0] + c * to[0], a * path.start[1] + b * path.bend[1] + c * to[1], 0, 0.8 + 0.2 * u];
    }
    var gone = at(220, 290, 0, 0.05);
    t['item-' + name] = [[0, { transform: at(path.start[0], path.start[1], 0, 0.75), opacity: 0 }, 'l']]
      .concat(along(fall, 0.55, caught - 0.05, 0.06, function (u) { return { opacity: Math.min(1, u * 6) }; }))
      .concat([[caught + 0.05, { transform: at(220, 288, 0, 0.35), opacity: 1 }, 'i'], [caught + 0.12, { transform: gone, opacity: 0 }, 'l'], [dur, { transform: gone, opacity: 0 }, 'l']]);
    var turns = (d || 0.5) * -720;
    t['spin-' + name] = [[0, REST, 'l'], [0.55, REST, 'l'], [caught + 0.12, at(0, 0, turns), 'hold'], [dur, REST, 'l']];
    t.rig = [[0, REST], [0.85, REST, 'o'], [1.0, at(0, -14, 0, 0.98, 1.03)], [1.2, REST, 'o'], [1.45, at(0, 0, 0, 1.1, 0.86), 'snap'],
      [1.58, at(0, lift * 0.45, 0, 0.92, 1.1), 'o'], [caught, at(0, lift), 'i'], [land, at(0, 0, 0, 0.97, 1.04), 'o'], [land + 0.1, at(0, 0, 0, 1.14, 0.84), 'o'],
      [2.65, at(0, 0, 0, 0.95, 1.06)], [2.9, at(0, 0, 0, 1.02, 0.98)], [3.15, REST]];
    // It looks toward the release: up for one from straight above.
    t.head = [[0, REST], [0.85, REST, 'o'], [1.0, at(4 * d, d ? -10 : -16, 4 * d)], [1.2, at(4 * d, -4, 3 * d)], [1.45, at(0, 14, 0), 'snap'], [1.6, at(0, -6, 0), 'o'],
      [caught, at(0, -4, -2)], [land, at(0, -10, 0), 'o'], [land + 0.12, at(0, 16, 0)], [2.75, at(0, 0, -6)], [3.05, at(0, 0, 5)], [3.35, at(0, 0, -3)], [3.65, at(0, 0, 1.5)], [3.95, REST]];
    t.features = [[0, REST], [0.85, REST, 'o'], [1.0, at(18 * d, d ? 0 : -10)], [1.4, at(18 * d, d ? 0 : -10)], [1.6, REST]];
    t.arms = [[0, REST], [1.2, REST], [1.45, at(0, 12), 'snap'], [1.75, at(0, -40, 0, 1.06), 'o'], [caught, at(0, -46, 0, 1.08)], [caught + 0.12, at(0, -30)], [land + 0.1, at(0, 14), 'o'], [2.7, at(0, -4)], [3.0, REST]];
    EARS.forEach(function (e) {
      var s = e[1];
      t[e[0]] = [[0, REST], [0.85, REST, 'o'], [1.0, at(0, 0, 30 * s)], [1.2, at(0, 0, 22 * s)], [1.45, at(0, 0, -10 * s), 'snap'], [1.62, at(0, 0, -22 * s), 'o'],
        [caught, at(0, 0, -8 * s)], [land + 0.1, at(0, 0, 26 * s), 'o'], [2.7, at(0, 0, -10 * s)], [3.0, at(0, 0, 6 * s)], [3.3, at(0, 0, -2 * s)], [3.6, REST]];
    });
    t.mouth = [[0, REST], [0.85, REST, 'o'], [1.0, at(0, 0, 0, 0.8, 1.5)], [1.4, at(0, 0, 0, 0.8, 1.3)], [1.6, REST], [land + 0.1, REST, 'o'], [2.6, at(0, 0, 0, 1.35, 1.6)], [3.6, at(0, 0, 0, 1.3, 1.5)], [4.0, REST]];
    t['marks-' + from] = [[0, { transform: at(0, 0, 0, 0.3), opacity: 0 }], [0.86, { transform: at(0, 0, 0, 0.3), opacity: 0 }, 'o'], [1.0, { transform: at(0, 0, 0, 1.1), opacity: 1 }], [1.3, { transform: REST, opacity: 1 }], [1.45, { transform: at(0, 0, 0, 1.05), opacity: 0 }]];
    t.flash = [[0, { opacity: 0 }], [caught, { opacity: 0 }, 'o'], [caught + 0.1, { opacity: 0.9 }], [2.6, { opacity: 0 }]];
    t.heart = [[0, REST], [caught + 0.05, REST, 'back'], [caught + 0.3, at(0, 0, 0, 1.6)], [2.7, at(0, 0, 0, 1.1)], [2.9, at(0, 0, 0, 1.35)], [3.3, REST]];
    t.cheekL = t.cheekR = [[0, REST], [land, REST, 'back'], [2.55, at(0, 0, 0, 1.4)], [3.9, at(0, 0, 0, 1.35)], [4.3, REST]];
    t.blush = [[0, { opacity: 0 }], [land + 0.05, { opacity: 0 }], [2.55, { opacity: 1 }], [3.9, { opacity: 1 }], [4.3, { opacity: 0 }]];
    [0.02, 0.1, 0.06, 0.16, 0.22].forEach(function (lag, i) {
      var s = land + lag;
      t['star-' + (i + 1)] = [[0, at(0, 0, 0, 0)], [s, at(0, 0, -40, 0), 'back'], [s + 0.3, at(0, 0, 0, 1.15)], [s + 0.9, at(0, -10, 20, 0.9), 'i'], [s + 1.4, at(0, -14, 30, 0)]];
    });
    function rise(u) { var w = Math.sin(u * Math.PI * 3); return [300 + 26 * w, 40 - 210 * u, 10 * w, 0.7 + 0.3 * Math.min(1, u * 5)]; }
    t.rise = [[0, { transform: at(300, 40, 0, 0.3), opacity: 0 }]].concat(along(rise, 2.5, 4.2, 0.1, function (u) { return { opacity: Math.min(1, u * 6) * (u < 0.6 ? 1 : (1 - u) / 0.4) }; }));
    return { dur: dur, tracks: t, releases: [release] };
  }

  // inhale: the sheep turns to one side (`d`), opens its mouth round and breathes in; streaks of air rush toward it and a release
  // far off wobbles, then is pulled in, faster as it nears, and vanishes into the mouth. Cheeks and head swell full, it holds, then gulps.
  function inhaleMoment(id, d, release) {
    var dur = 7.5, t = {}, name = id + '-1', mouthX = 226 + 30 * d, x0 = d > 0 ? 660 : -170 + 70, got = 2.7;
    faceToward(t, d, [0.6, 1.0], [4.2, 4.7], 30);
    t.head = [[0, REST], [0.6, REST], [1.0, at(4 * d, 0, 2 * d)], [1.25, at(-10 * d, -6, -3 * d), 'o'], [got - 0.1, at(-14 * d, -8, -4 * d), 'o'],
      [got + 0.08, at(6 * d, 4, 2 * d, 1.07, 1.05), 'o'], [3.0, at(4 * d, 0, d, 1.06, 1.05)], [3.3, at(4 * d, -2, d, 1.07, 1.04)], [3.55, at(4 * d, 0, d, 1.06, 1.05), 'i'],
      [3.75, at(2 * d, 12, 0), 'o'], [4.05, at(0, -5, 0)], [4.4, REST]];
    // The mouth opens round (the wide mouth, drawn smaller), widens as it pulls, and shuts on the release.
    var small = at(0, 0, 0, 0.3, 0.1), round = at(0, 0, 0, 0.52, 0.62), wider = at(0, 0, 0, 0.6, 0.78);
    t.mouth = [[0, { opacity: 1 }], [1.0, { opacity: 1 }, 'l'], [1.06, { opacity: 0 }], [3.97, { opacity: 0 }, 'l'], [4.05, { opacity: 1 }]];
    t.gape = [[0, { opacity: 0, transform: small }], [1.0, { opacity: 0, transform: small }, 'l'], [1.04, { opacity: 1, transform: small }, 'o'], [1.3, { opacity: 1, transform: round }],
      [got - 0.2, { opacity: 1, transform: wider }], [got, { opacity: 1, transform: at(0, 0, 0, 0.5, 0.1) }, 'l'], [got + 0.03, { opacity: 0, transform: small }]];
    t.munch = munching(got + 0.03, [], 3.97);
    t.cheekL = t.cheekR = [[0, REST], [got, REST, 'back'], [got + 0.2, at(0, 0, 0, 1.9, 1.75)], [3.6, at(0, 0, 0, 1.8, 1.7), 'o'], [3.85, REST]];
    t.eyes = [[0, REST], [1.2, REST], [1.4, at(0, 2, 0, 1.04, 0.82)], [got, at(0, 2, 0, 1.04, 0.82)], [got + 0.1, at(0, 4, 0, 1.1, 0.6)], [3.7, at(0, 4, 0, 1.1, 0.6)], [4.0, REST]];
    t.rig = [[0, REST], [1.0, REST], [1.6, at(0, -4, 0, 1.02, 1.03)], [got, at(0, -6, 0, 1.03, 1.04)], [got + 0.12, at(0, 0, 0, 1.06, 0.97), 'o'], [3.55, at(0, 0, 0, 1.05, 0.98)]].concat(swallow(3.75).slice(1));
    EARS.forEach(function (e) {
      var s = e[1];
      // The ears stream toward the pull, then flick after the gulp.
      t[e[0]] = [[0, REST], [1.2, REST], [1.6, at(0, 0, (d * s > 0 ? -10 : 10) * s)], [got, at(0, 0, (d * s > 0 ? -14 : 14) * s)], [got + 0.15, at(0, 0, 12 * s), 'o'],
        [3.75, at(0, 0, -6 * s), 'o'], [4.05, at(0, 0, 18 * s)], [4.35, at(0, 0, -5 * s)], [4.65, REST]];
    });
    // The release: still far off, it wobbles against the pull, then comes in faster and faster, shrinking and turning.
    var wobble = [[0, { transform: at(x0, 290, 0), opacity: 0 }, 'l'], [1.2, { transform: at(x0, 290, 0), opacity: 0 }, 'o'], [1.45, { transform: at(x0, 290, -8 * d), opacity: 1 }]];
    [1.65, 1.85].forEach(function (s, i) { wobble.push([s, { transform: at(x0 - d * (8 + 10 * i), 290, (i % 2 ? -10 : 6) * d), opacity: 1 }, 'io']); });
    function pull(u) { var w = Math.pow(u, 2.4); return [x0 - d * 18 + (mouthX - x0 + d * 18) * w, 290 + 14 * w, -d * 300 * w, 1 - 0.65 * w]; }
    t['item-' + name] = wobble.concat(along(pull, 2.0, got - 0.02, 0.07, function () { return { opacity: 1 }; }),
      [[got + 0.02, { transform: at(mouthX, 304, 0, 0.2), opacity: 0 }, 'l'], [dur, { transform: at(mouthX, 304, 0, 0.2), opacity: 0 }, 'l']]);
    t['spin-' + name] = [[0, REST], [dur, REST]];
    // Four streaks of air, staggered, each rushing from beside the release to the mouth twice over.
    [[0, 300], [0.12, 245], [0.24, 350], [0.36, 275]].forEach(function (w, i) {
      var keys = [[0, { transform: at(x0, w[1], 0, 1, 1), opacity: 0 }, 'l']];
      [1.3, 1.95].forEach(function (s0) {
        var s = s0 + w[0];
        keys.push([s, { transform: at(x0 - d * 30, w[1], 0, 0.6, 1), opacity: 0 }, 'i'], [s + 0.12, { transform: at(x0 - d * 90, w[1] + (300 - w[1]) * 0.2, 0, 1, 1), opacity: 1 }, 'i'],
          [s + 0.5, { transform: at(mouthX + d * 70, 300 + (w[1] - 300) * 0.25, 0, 0.5, 1), opacity: 0 }, 'l']);
      });
      t['wind-' + (i + 1)] = keys;
    });
    t.heart = [[0, REST], [3.95, REST, 'back'], [4.2, at(0, 0, 0, 1.5)], [4.7, REST]];
    return { dur: dur, tracks: t, releases: [release] };
  }

  // bop: the heart on its player starts to pulse and the sheep bops to the music in its headphones: nods, sways, two little hops,
  // ears flapping. No release.
  function bopMoment() {
    var dur = 7, t = {}, beat = 0.42, first = 0.9, beats = [];
    for (var b = 0; b < 8; b++) beats.push(first + b * beat);
    var head = [[0, REST], [first - 0.3, REST]], rig = [[0, REST], [first - 0.2, REST]], heart = [[0, REST], [first - 0.42, REST]], ears = { earL: [[0, REST], [first - 0.2, REST]], earR: [[0, REST], [first - 0.2, REST]] };
    beats.forEach(function (s, i) {
      var side = i % 2 ? -1 : 1, hop = i === 2 || i === 3 || i === 6;
      head.push([s, at(3 * side, 8, 6 * side), 'o'], [s + beat * 0.55, at(2 * side, -3, 4 * side), 'io']);
      // A hop leaves the ground on its beat and lands on the next one, squashing there.
      if (hop) rig.push([s, at(0, 0, 0, 1.06, 0.92), 'o'], [s + 0.16, at(0, -34, 0, 0.95, 1.06), 'i'], [s + beat - 0.03, REST]);
      else rig.push([s, at(0, 0, 0, 1.03, 0.97), 'o'], [s + beat * 0.6, REST, 'io']);
      heart.push([s - 0.02, REST, 'back'], [s + 0.1, at(0, 0, 0, 1.3)], [s + beat - 0.04, REST]);
      EARS.forEach(function (e) { ears[e[0]].push([s + 0.06, at(0, 0, (hop ? 18 : 9) * e[1]), 'o'], [s + beat * 0.7, at(0, 0, -4 * e[1]), 'io']); });
    });
    var end = beats[7] + beat;
    t.head = head.concat([[end, at(0, 4, 0)], [end + 0.35, REST]]);
    t.rig = rig.concat([[end + 0.1, REST]]);
    t.heart = heart.concat([[end + 0.3, REST]]);
    EARS.forEach(function (e) { t[e[0]] = ears[e[0]].concat([[end + 0.25, at(0, 0, 4 * e[1])], [end + 0.6, REST]]); });
    t.mouth = [[0, REST], [first - 0.2, REST, 'o'], [first + 0.1, at(0, 0, 0, 1.25, 1.4)], [end, at(0, 0, 0, 1.25, 1.4)], [end + 0.3, REST]];
    t.eyes = [[0, REST], [first, REST], [first + 0.15, at(0, 3, 0, 1.05, 0.78)], [end, at(0, 3, 0, 1.05, 0.78)], [end + 0.3, REST]];
    t.cheekL = t.cheekR = [[0, REST], [first, REST], [first + 0.3, at(0, 0, 0, 1.2)], [end, at(0, 0, 0, 1.2)], [end + 0.3, REST]];
    return { dur: dur, tracks: t, releases: [] };
  }

  // roll: the sheep tucks its head, springs up and rolls once on the spot, lands with a squash, and wobbles back upright. No release.
  function rollMoment() {
    var dur = 6, t = {}, go = 1.15, down = 1.85;
    t.roll = [[0, REST], [go, REST, 'o'], [go + 0.18, at(0, -60, 70), 'l'], [go + 0.35, at(0, -90, 180), 'l'], [go + 0.52, at(0, -60, 290), 'i'], [down, at(0, 0, 360), 'hold'], [dur, REST]];
    t.rig = [[0, REST], [0.7, REST], [go, at(0, 0, 0, 1.1, 0.86), 'snap'], [go + 0.12, at(0, 0, 0, 0.94, 1.08), 'o'], [go + 0.3, at(0, 0, 0, 0.96, 0.96)],
      [down - 0.05, at(0, 0, 0, 0.98, 1.02), 'i'], [down + 0.1, at(0, 0, 0, 1.14, 0.84), 'o'], [2.2, at(0, 0, 0, 0.95, 1.06)], [2.45, at(0, 0, 0, 1.02, 0.98)], [2.7, REST]];
    t.head = [[0, REST], [0.7, REST], [go, at(0, 24, 0, 1.03, 0.95), 'snap'], [go + 0.3, at(0, 18, 0)], [down - 0.05, at(0, 10, 0), 'i'], [down + 0.12, at(0, 18, 0), 'o'],
      [2.3, at(0, -8, 0)], [2.6, at(0, 0, -5)], [2.95, at(0, 0, 4)], [3.3, at(0, 0, -2)], [3.6, REST]];
    t.arms = [[0, REST], [0.7, REST], [go, at(0, -14, 0, 0.94), 'snap'], [down - 0.05, at(0, -14, 0, 0.94)], [down + 0.15, at(0, 10), 'o'], [2.4, REST]];
    EARS.forEach(function (e) {
      var s = e[1];
      t[e[0]] = [[0, REST], [0.7, REST], [go, at(0, 0, -16 * s), 'snap'], [go + 0.35, at(0, 0, 24 * s), 'l'], [down, at(0, 0, -12 * s), 'o'], [down + 0.15, at(0, 0, 28 * s), 'o'],
        [2.4, at(0, 0, -10 * s)], [2.7, at(0, 0, 6 * s)], [3.0, at(0, 0, -2 * s)], [3.3, REST]];
    });
    t.eyes = [[0, REST], [0.8, REST], [go, at(0, 4, 0, 1.08, 0.6)], [down + 0.3, at(0, 4, 0, 1.08, 0.6)], [2.4, REST]];
    t.mouth = [[0, REST], [down, REST, 'o'], [down + 0.2, at(0, 0, 0, 1.3, 1.5)], [3.2, at(0, 0, 0, 1.25, 1.4)], [3.5, REST]];
    t.cheekL = t.cheekR = [[0, REST], [down, REST, 'back'], [down + 0.2, at(0, 0, 0, 1.3)], [3.2, at(0, 0, 0, 1.25)], [3.5, REST]];
    return { dur: dur, tracks: t, releases: [] };
  }

  // The sequence, one moment after another, then round again. It never plays the same moment twice running, and the releases differ each time.
  var IMPORT_SHEEP_SEQUENCE = [
    function (id) { return gobbleMoment(id, 1, [{ kind: 'cover', tone: 'blue' }, { kind: 'vinyl', tone: 'gold' }, { kind: 'cd', tone: 'pink' }]); },
    function (id) { return catchMoment(id, 'left', { kind: 'vinyl', tone: 'sky' }); },
    function () { return bopMoment(); },
    function (id) { return gobbleMoment(id, -1, [{ kind: 'cd', tone: 'mint' }, { kind: 'cover', tone: 'coral' }, { kind: 'vinyl', tone: 'orchid' }]); },
    function (id) { return inhaleMoment(id, 1, { kind: 'sleeve', tone: 'lime', disc: 'blue' }); },
    function (id) { return catchMoment(id, 'top', { kind: 'cd', tone: 'coral' }); },
    function () { return rollMoment(); },
    function (id) { return catchMoment(id, 'right', { kind: 'cover', tone: 'orchid' }); }
  ];
  var RELEASE_SIZE = { gobble: 44, catch: 50, inhale: 54 };
  // What each moving part looks like at rest, for the moments that leave it alone.
  function restOf(part) {
    if (/^item-/.test(part) || /^(flash|blush|rise|wind-|marks-)/.test(part)) return { opacity: 0 };
    if (/^star-/.test(part)) return { transform: at(0, 0, 0, 0) };
    return { transform: REST };
  }
  var importSheepMoments = null;
  function readMoments() {
    if (!importSheepMoments) importSheepMoments = IMPORT_SHEEP_SEQUENCE.map(function (make, i) { return make(i + 1); });
    return importSheepMoments;
  }
  function percent(n) { return (Math.round(n * 10000) / 10000) + '%'; }
  // The whole sequence as CSS: one loop of keyframes per part, run only where the person hasn't asked for reduced motion.
  function importSheepCss() {
    var moments = readMoments(), total = 0, parts = {};
    moments.forEach(function (m) { total += m.dur; Object.keys(m.tracks).forEach(function (p) { parts[p] = true; }); });
    var frames = [], runs = [];
    Object.keys(parts).forEach(function (part) {
      var keys = [], start = 0;
      moments.forEach(function (m, mi) {
        var own = m.tracks[part] ? m.tracks[part].map(function (k) { return [k[0], typeof k[1] === 'string' ? { transform: k[1] } : k[1], k[2] || 'io']; }) : [[0, restOf(part), 'l']];
        own.sort(function (a, b) { return a[0] - b[0]; });
        if (own[own.length - 1][0] < m.dur) own.push([m.dur, own[0][1], 'l']);
        // A moment's last key is the next moment's first, so it's left out except at the very end.
        own.forEach(function (k, i) { if (i < own.length - 1 || mi === moments.length - 1) keys.push([start + k[0], k[1], k[2]]); });
        start += m.dur;
      });
      var name = 'bae-isheep-' + part;
      frames.push('@keyframes ' + name + ' { ' + keys.map(function (k) {
        var decl = Object.keys(k[1]).map(function (p) { return p + ': ' + k[1][p]; }).join('; ');
        return percent(k[0] / total * 100) + ' { ' + decl + '; animation-timing-function: ' + ISHEEP_EASE[k[2]] + '; }';
      }).join(' ') + ' }');
      runs.push('.bae-isheep:not(.bae-isheep-still) .bae-isheep-' + part + ' { animation: ' + name + ' ' + total + 's infinite; }');
    });
    return frames.join('\n') + '\n@media (prefers-reduced-motion: no-preference) {\n' + runs.join('\n') + '\n}\n';
  }
  // The keyframes go into the page once, the first time a sheep is drawn.
  function addImportSheepCss() {
    if (document.getElementById('bae-isheep-motion')) return;
    var style = document.createElement('style');
    style.id = 'bae-isheep-motion';
    style.textContent = importSheepCss();
    document.head.appendChild(style);
  }

  // The sheep on an empty Import screen. Every few seconds it plays one moment from a fixed sequence, then rests: it gobbles a line
  // of releases from the left or the right, catches one falling from above, breathes one in, bops to its music, or rolls on the spot.
  // `size` is the sheep's width. With reduced motion, or with `still`, it holds a still pose: the sheep at rest with a release beside it.
  function ImportSheep(props) {
    var size = Number(props.size || 140);
    var prefix = React.useState(function () { importSheepCount += 1; return 'bae-isheep' + importSheepCount + '-'; })[0];
    var parts = readSheepParts();
    addImportSheepCss();
    function part(name) { return h('g', { key: name, dangerouslySetInnerHTML: { __html: ownIds(parts[name], prefix) } }); }
    function moving(name, children) { return h('g', { key: name, className: 'bae-isheep-' + name }, children); }
    var releases = [];
    readMoments().forEach(function (m, mi) {
      var kind = m.tracks['wind-1'] ? 'inhale' : m.tracks.gape ? 'gobble' : 'catch';
      m.releases.forEach(function (r, ri) { releases.push(h(Incoming, Object.assign({ key: 'r' + mi + '-' + ri, id: (mi + 1) + '-' + (ri + 1), size: RELEASE_SIZE[kind] }, r))); });
    });
    function star(i, p, fill) { return h('g', { key: i, transform: 'translate(' + p[0] + ' ' + p[1] + ') scale(' + p[2] + ')' }, h('path', { className: 'bae-isheep-star-' + (i + 1), d: 'M0-34C4-8 8-4 34 0C8 4 4 8 0 34C-4 8-8 4-34 0C-8-4-4-8 0-34Z', strokeWidth: 7, style: { fill: fill } })); }
    var accentLine = { fill: 'none', strokeWidth: 14, style: { stroke: 'var(--accent)' } };
    var head = moving('head', [
      moving('earL', part('earL')), moving('earR', part('earR')),
      part('face'),
      // The features stay inside the face as they slide with its turn.
      h('g', { key: 'in-face', clipPath: 'url(#' + prefix + 'in-face)' }, moving('features', [
        moving('cheekL', part('cheekL')), moving('cheekR', part('cheekR')),
        // Blush lines across the cheeks, for after a catch.
        h('g', { key: 'blush', className: 'bae-isheep-blush', fill: 'none', strokeWidth: 6, stroke: '#e0508f' }, h('path', { d: 'M108 262l-10 14M124 262l-10 14M140 262l-10 14M322 273l-10 14M338 273l-10 14M354 273l-10 14' })),
        moving('mouth', part('mouth')),
        // The mouth opened wide to chomp, in the sheep's own mouth colours, and the closed mouth that munches.
        h('g', { key: 'gape', className: 'bae-isheep-gape' },
          h('path', { d: 'M184 266C184 258 268 258 268 266C270 312 246 338 226 338C206 338 182 312 184 266Z', fill: PLUM, strokeWidth: 6 }),
          h('path', { d: 'M198 322C206 304 246 304 254 322C246 334 206 334 198 322Z', fill: '#ff81bb', stroke: 'none' })),
        h('path', { key: 'munch', className: 'bae-isheep-munch', d: 'M196 270q15 20 30 4q15 16 30-4', fill: 'none', strokeWidth: 11 }),
        moving('eyes', part('eyes'))])),
      part('wool'),
      part('earcupR')]);
    return h('svg', { className: cx('bae-isheep', props.still && 'bae-isheep-still'), viewBox: '-170 -215 924 767', width: Math.round(size * 924 / 584), height: Math.round(size * 767 / 584), 'aria-hidden': true },
      h('defs', { dangerouslySetInnerHTML: { __html: ownIds(parts.defs, prefix) } }),
      h('g', { stroke: PLUM, strokeLinejoin: 'round', strokeLinecap: 'round' },
        moving('roll', moving('rig', [
          moving('head', [part('headband'), part('earcupL')]),
          part('body'),
          head,
          moving('arms', [part('player'),
            // The screen lighting up as a caught release goes in.
            h('g', { key: 'flash', className: 'bae-isheep-flash', dangerouslySetInnerHTML: { __html: ownIds(parts.screen, prefix) } }),
            moving('heart', part('heart')), part('bubbles'), part('handL'), part('handR')])])),
        // Streaks of air for the breath in.
        [1, 2, 3, 4].map(function (i) { return h('g', { key: 'wind' + i, className: 'bae-isheep-wind-' + i }, h('path', { d: 'M-70 0Q0-18 70 0', fill: 'none', strokeWidth: 13, style: { stroke: 'var(--accent)' } })); }),
        releases,
        // Marks of surprise beside the head, on the side a release comes from, or above it.
        h('g', Object.assign({ key: 'mr', className: 'bae-isheep-marks-right' }, accentLine), h('path', { d: 'M452 -6l22-40M492 22l40-22M500 72l44 2' })),
        h('g', Object.assign({ key: 'ml', className: 'bae-isheep-marks-left' }, accentLine), h('path', { d: 'M28 -6l-22-40M-12 22l-40-22M-20 72l-44 2' })),
        h('g', Object.assign({ key: 'mt', className: 'bae-isheep-marks-top' }, accentLine), h('path', { d: 'M180 -20l-14-40M240 -30v-44M300 -20l14-40' })),
        // Stars that burst out after a catch, and the heart that floats up.
        h('g', { key: 'stars' }, [[500, 110, 1.5], [-30, 40, 1.2], [590, 330, 1.1], [-70, 310, 1.35], [250, -150, 1]].map(function (p, i) { return star(i, p, i % 2 ? 'var(--accent-gold-fill)' : 'var(--accent-pink-fill)'); })),
        h('g', { key: 'rise', className: 'bae-isheep-rise' }, h('path', { d: 'M0 14C-14 2-26-8-26-20C-26-34-8-38 0-24C8-38 26-34 26-20C26-8 14 2 0 14Z', strokeWidth: 8, style: { fill: 'var(--accent-pink-fill)' } }))));
  }
