  // ---- Library and playback ----

  // A stand-in cover: each title gets its own two-color square, so boards read like a library.
  // `missing` is a cover the release doesn't have: a plain tile, no glyph.
  var boardArtwork = /* board artwork */ {};
  function hueOf(s) { var x = 0; s = String(s || ''); for (var i = 0; i < s.length; i++) x = (x * 31 + s.charCodeAt(i)) % 360; return x; }
  // `size` and `radius` are pixels or a token's name (`size-row-artwork`, `radius-artwork`).
  function Artwork(props) {
    var size = props.size == null ? null : measure(props.size);
    var name = props.seed || props.title;
    var style = { borderRadius: measure(props.radius != null ? props.radius : 'radius-cover') };
    if (size) { style.width = size; style.height = size; }
    if (props.round) style.borderRadius = '50%';
    if (!props.missing) {
      if (props.hue == null && Object.hasOwn(boardArtwork, name)) {
        style.backgroundImage = 'url(' + boardArtwork[name] + ')';
        style.backgroundSize = '100% 100%';
      } else {
        var hue = props.hue != null ? Number(props.hue) : hueOf(name);
        style.backgroundImage = 'linear-gradient(135deg, hsl(' + hue + ' 42% 52%), hsl(' + ((hue + 48) % 360) + ' 46% 24%))';
      }
    }
    return h('div', { className: cx('bae-art', props.missing && 'bae-art-missing', props.shadow && 'bae-art-shadow-' + props.shadow, props.ring && 'bae-art-ring', props.dimmed && 'bae-art-dimmed', props.className), style: style, role: props.label ? 'img' : null, 'aria-label': props.label || null, 'aria-hidden': props.label ? null : true }, props.children);
  }

  // A person's picture, round: their image when they have one, or the initials of their name on a tint. `image` stands for the picked image (the board draws it as artwork seeded by it). `android`: the initials centred as Compose centres them, on CSS's line. On the Mac they sit on SwiftUI's line, which the board draws exactly (`macInitials`).
  function Avatar(props) {
    var s = Number(props.size || 32), name = String(props.name || '?');
    var ini = name.split(/\s+/).map(function (p) { return p[0]; }).join('').slice(0, 2).toUpperCase();
    if (props.image) return h(Artwork, { title: props.image, size: s, round: true });
    return h('span', { className: 'bae-avatar', 'aria-hidden': true, style: { width: s, height: s, fontSize: s * 0.4 } }, props.android ? ini : macInitials(ini, s));
  }
  // SwiftUI sets a line of SF Pro with its baseline at the font's ascent (0.9668 of the size) rounded to a whole point, in a box that adds the descent (0.2109) rounded up, and the avatar centres that box. The board places the baseline there, where CSS's own line would round the font's metrics another way.
  function macInitials(ini, s) {
    var f = s * 0.4, ascent = Math.round(f * 0.9668), descent = Math.ceil(f * 0.2109);
    return h('svg', { className: 'bae-avatar-initials', width: s, height: s, viewBox: '0 0 ' + s + ' ' + s },
      h('text', { x: s / 2, y: s / 2 + (ascent - descent) / 2, textAnchor: 'middle', fill: 'currentColor' }, ini));
  }

  // A playlist's picture: the covers of its first four different releases in a 2×2 square, as playlists show on both platforms; with fewer than four, its first entry's cover; empty, the missing tile. `covers` lists the releases' titles in the playlist's order, each once. One cover alone would make a playlist look like that album, and two playlists that start on one album alike.
  function PlaylistPicture(props) {
    var covers = String(props.covers || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean);
    var size = measure(props.size || 48), radius = measure(props.radius != null ? props.radius : 'radius-artwork');
    if (covers.length < 4) return h(Artwork, { title: covers[0], size: props.size || 48, radius: props.radius != null ? props.radius : 'radius-artwork', missing: !covers.length });
    // Four covers, each filling a quarter of the picture's two-by-two grid.
    return h('div', { className: 'bae-playlist-pic', style: { width: size, height: size, borderRadius: radius }, 'aria-hidden': true },
      covers.slice(0, 4).map(function (t, i) { return h(Artwork, { key: i, title: t, radius: 0 }); }));
  }

  // A Mac glyph (`set: 'mac'`) is the SF Symbol `name` (SF) at `size`, an icon role or a number of points, in `weight` where the app draws it in a weight the table names after a colon, or the weight its own entry is traced in. It takes the room of the frame SwiftUI lays the image out in at that size (SF's `frames`), the symbol centred in it, and its own box is the symbol's ink, as VoiceOver and the shots measure an image.
  // `frame` ([width, height] in points, null for the symbol's own), or `square` (the size both ways): the frame the app sets around the symbol, which is then its room, the symbol's own frame centred in it with its corner on a device pixel, as SwiftUI places it. `contentShape`: the app gives the symbol a content shape, which makes the room its own box too. `text`: the symbol sits in a run of text, as Text(Image) sets it, and takes its ink's room alone.
  // `point: 'forward'` or `point: 'backward'` turns the `down` chevron to point that way (chevron.forward, chevron.backward). `button`: the glyph is a plain button by that name, with the glyph's box and room, as a SwiftUI button around one symbol is; `className` then styles the button. `label`: the glyph's name for a screen reader, as an image; without one, a screen reader skips it. `turn`: the degrees the glyph turns about its frame's centre, as SwiftUI's rotationEffect turns an image, with its room and box left as they are. Any other set draws the Material symbol (MSymbol).
  function Glyph(props) {
    var turned = props.point === 'forward' || props.point === 'backward';
    if (props.point != null && !(turned && props.name === 'down' && props.set === 'mac')) throw new Error('Glyph turns only the Mac `down` chevron, to point `forward` or `backward`');
    if (props.set !== 'mac') return h(MSymbol, { name: props.name, size: props.size, className: cx('bae-glyph', props.className), fill: 'currentColor', label: props.label });
    var base = turned ? props.point : props.name, key = base + (props.weight ? ':' + props.weight : '');
    var sf = SF[key] || (props.weight && SF[base] && SF[base].weight === props.weight ? SF[base] : null);
    if (!sf) throw new Error('SF has no symbol `' + key + '`');
    // Boxes as left, top, right, bottom, in points at 24 around the frame's centre (12, 12); k turns them into points at the glyph's size.
    var points = iconPoints(props.size), k = points / 24;
    // Along axis 0 (width) or 1 (height), in points: the frame SwiftUI lays the symbol out in at this size, and the frame the caller sets, if any.
    var symbolFrame = function (axis) {
      var at = sf.frames[Math.round(points * 100) / 100];
      if (!at) throw new Error('SF has no frame for `' + key + '` at ' + points + ' points: add the frame SwiftUI lays it out in at that size');
      return at[axis];
    };
    var callerFrame = function (axis) { return props.square ? points : props.frame && props.frame[axis] != null ? props.frame[axis] : null; };
    // The frame's side, in points at 24.
    var side = function (axis) { var set = callerFrame(axis); return (set != null ? set : symbolFrame(axis)) / k; };
    // In a frame the caller sets, SwiftUI centres the symbol's own frame there with its corner on a device pixel (half a point), which moves the ink up to a quarter point off the centre; in points at 24.
    var shift = function (axis) {
      var set = callerFrame(axis);
      if (set == null) return 0;
      var offset = (set - symbolFrame(axis)) / 2;
      return (Math.round(offset * 2) / 2 - offset) / k;
    };
    var ink = props.text ? sf.ink : [sf.ink[0] + shift(0), sf.ink[1] + shift(1), sf.ink[2] + shift(0), sf.ink[3] + shift(1)];
    var room = props.text ? ink : [12 - side(0) / 2, 12 - side(1) / 2, 12 + side(0) / 2, 12 + side(1) / 2];
    var own = props.contentShape ? room : ink;
    var margin = props.contentShape ? null : [own[1] - room[1], room[2] - own[2], room[3] - own[3], own[0] - room[0]].map(function (m) { return thousandths(m * k) + 'px'; }).join(' ');
    var size = { width: thousandths((own[2] - own[0]) * k), height: thousandths((own[3] - own[1]) * k), overflow: 'visible' };
    var view = [own[0], own[1], own[2] - own[0], own[3] - own[1]].map(thousandths).join(' ');
    var path = h('path', { d: sf.d });
    // The frame's centre is (12, 12), which sits (12 - own left, 12 - own top) into the glyph's box.
    var turn = props.turn == null ? null : { transform: 'rotate(' + props.turn + 'deg)', transformOrigin: thousandths((12 - own[0]) * k) + 'px ' + thousandths((12 - own[1]) * k) + 'px' };
    if (props.button) return h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-glyph-button', props.className), 'aria-label': props.button, title: props.button, style: Object.assign({ margin: margin }, size) },
      h('svg', { viewBox: view, className: 'bae-glyph', fill: 'currentColor', style: Object.assign({}, size, turn), 'aria-hidden': true }, path));
    var named = props.label ? { role: 'img', 'aria-label': props.label } : { 'aria-hidden': true };
    return h('svg', Object.assign({ viewBox: view, className: cx('bae-glyph', props.className), fill: 'currentColor', style: Object.assign({ margin: margin }, size, turn) }, named), path);
  }
  function thousandths(v) { return Math.round(v * 1000) / 1000; }

  // `large` is Android's full player's bar: a tall track with a tall thumb, and no held parts.
  // Position and the parts this device holds (`held`, "0-0.4,0.6-0.7"), drawn faint under what has played.
  function heldRanges(held) {
    if (held == null || held === '') return [];
    if (typeof held === 'number') return [[0, held]];
    return String(held).split(',').map(function (r) { var p = r.split('-'); return [Number(p[0]), Number(p[1])]; });
  }
  // A time as m:ss or h:mm:ss, in seconds.
  function seconds(t) { return String(t || '0').split(':').reduce(function (n, p) { return n * 60 + Number(p); }, 0); }
  // The line fills the elapsed time's share of the duration.
  function SeekLine(prefix) {
    return function (props) {
      var total = seconds(props.duration), pos = total ? seconds(props.elapsed) / total : 0;
      return h('div', { className: cx('bae-seek', prefix + '-seek', props.off && 'bae-seek-off', props.large && prefix + '-seek-large') },
        h('span', { className: 'bae-seek-time' }, props.elapsed || '0:00'),
        h('div', { className: 'bae-seek-track', role: 'slider', 'aria-label': 'Position', 'aria-valuenow': Math.round(pos * 100), 'aria-disabled': props.off || undefined },
          heldRanges(props.held).map(function (r, i) { return h('div', { key: i, className: 'bae-seek-held', style: { left: (r[0] * 100) + '%', width: ((r[1] - r[0]) * 100) + '%' } }); }),
          props.off ? null : h('div', { className: 'bae-seek-played', style: { width: (pos * 100) + '%' } }),
          prefix === 'bae-a' && !props.off ? h('div', { className: 'bae-seek-thumb', style: { left: (pos * 100) + '%' } }) : null),
        h('span', { className: 'bae-seek-time' }, props.duration || '0:00'));
    };
  }
  var MacSeekBar = SeekLine('bae-m');
  var AndroidSeekBar = SeekLine('bae-a');

  // ---- macOS library ----

  function MacIconButton(props) {
    return h('button', { type: 'button', disabled: props.off ? true : null, className: cx('bae-m-iconbtn', props.active && 'bae-m-iconbtn-on', props.faint && 'bae-m-iconbtn-faint', props.small && 'bae-m-iconbtn-small', props.boxed && 'bae-m-iconbtn-boxed', props.overlay && 'bae-m-iconbtn-overlay', props.off && 'bae-off'), 'aria-label': props.label || props.icon, 'aria-haspopup': props.menu ? 'menu' : null, 'aria-expanded': props.menu ? !!props.active : null, title: props.help || props.label },
      h(Glyph, { set: 'mac', name: props.icon, size: props.size || 'large' }));
  }

  // A row's ⋯ button with the row's actions in its menu: a row offers its actions here, not as a button of each. `open` draws the menu under it, the button as it is while closed, as AppKit leaves a menu's button, `items` its entries ("-" for a line between groups), `highlighted` the one under the pointer; `off` dims it while none of its actions can run. `boxed` fills it as a push button, for a page's header; `overlay` makes it a round button on a dark fill, for a place on a cover.
  function MacMoreMenu(props) {
    var btn = h(MacIconButton, { icon: 'ellipsis', size: 'medium', label: props.label || 'More', menu: true, off: props.off, boxed: props.boxed, overlay: props.overlay });
    if (!props.open) return btn;
    return h('span', { className: 'bae-m-moremenu' }, btn, h('div', { className: 'bae-m-moremenu-pop' }, h(MacMenu, null, list(props.items).map(function (t, i) {
      return t === '-' ? h(MacMenuItem, { key: i, divider: true }) : h(MacMenuItem, { key: i, highlighted: t === props.highlighted }, t);
    }))));
  }
  // The same on Android: the row's ⋮, its menu under it when `open`; `off` dims it while none of its actions can run.
  function AndroidMoreMenu(props) {
    var btn = h(AndroidIconButton, { icon: 'more', label: 'More', off: props.off, menu: true });
    if (!props.open) return btn;
    return h('span', { className: 'bae-a-moremenu' }, btn, h(AndroidMenu, { top: 44, right: 0 }, list(props.items).map(function (t, i) {
      return t === '-' ? h(AndroidMenuItem, { key: i, divider: true }) : h(AndroidMenuItem, { key: i, pressed: t === props.highlighted }, t);
    })));
  }

  function MacPlayPause(props) {
    // A button named by what it does, as the app's.
    var button = h('button', { type: 'button', className: 'bae-m-playpause', 'aria-label': props.playing ? 'Pause' : 'Play' },
      h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 'large' }));
    if (!props.waiting) return button;
    // While the track waits on storage, the arc turns over the button: a progress indicator of its own beside it, which VoiceOver reads as Downloading the track… before the button, as the app's overlay.
    return h('span', { className: 'bae-m-playpause-wait' },
      h('svg', { className: 'bae-waitarc', width: 48, height: 48, viewBox: '0 0 48 48', role: 'progressbar', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })),
      button);
  }

  // Shuffle, previous, play and pause, next, repeat; `shuffle` and `repeat` light theirs while on, `waiting` draws the arc around play while the track waits on storage.
  function MacTransport(props) {
    return h('div', { className: 'bae-m-transport' },
      h(MacIconButton, { icon: 'shuffle', label: props.shuffle ? 'Turn off shuffle' : 'Shuffle', active: props.shuffle }),
      h(MacIconButton, { icon: 'previous', label: 'Previous' }),
      h(MacPlayPause, { playing: props.playing, waiting: props.waiting }),
      h(MacIconButton, { icon: 'next', label: 'Next' }),
      h(MacIconButton, { icon: 'repeat', label: 'Repeat', active: props.repeat }));
  }

  // Mute and the volume slider, filled to `volume` (0 to 1). `noVolume`: playing on a receiver that takes no volume from bae (a UPnP renderer with no volume control): no Mute and no slider, and a line naming where the volume is set.
  function MacVolumeSlider(props) {
    if (props.noVolume) return h('span', { className: 'bae-m-np-novolume' }, 'Volume is set on ' + props.noVolume);
    return h('div', { className: 'bae-m-volumeslider' },
      h(MacIconButton, { icon: 'speaker', label: 'Mute' }),
      h('div', { className: 'bae-m-volume', role: 'slider', 'aria-label': 'Volume', 'aria-valuenow': Math.round((props.volume != null ? Number(props.volume) : 0.7) * 100) }, h('div', { style: { width: ((props.volume != null ? Number(props.volume) : 0.7) * 100) + '%' } })));
  }

  function MacNowPlayingBar(props) {
    var none = !props.title;
    return h('div', { className: 'bae-m-npbar' },
      // The playing track: one button, its cover beside its title and artist, named as VoiceOver reads the merged track.
      h('div', { className: 'bae-m-np-left' }, none ? null : h('button', { type: 'button', className: 'bae-plainbtn bae-m-nptrack', 'aria-label': props.artist ? 'Now playing, ' + props.title + ' by ' + props.artist : props.title },
        h(Artwork, { title: props.album || props.title, size: 'size-bar-artwork', radius: 'radius-artwork', shadow: 'bar', missing: props.noCover }),
        h('div', { className: 'bae-m-np-text', 'aria-hidden': true }, h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist)))),
      h('div', { className: 'bae-m-np-center' },
        h(MacTransport, { playing: props.playing, waiting: props.waiting, shuffle: props.shuffle, repeat: props.repeat }),
        h(MacSeekBar, { held: props.held, elapsed: props.elapsed, duration: props.duration, off: none })),
      h('div', { className: 'bae-m-np-right' },
        h(MacIconButton, { icon: 'cast', label: 'Play on…', help: props.casting ? 'Playing on ' + props.casting : null, active: !!props.casting }),
        h(MacIconButton, { icon: 'queue', label: 'Queue', active: props.queueOpen }),
        h(MacVolumeSlider, { volume: props.volume, noVolume: props.noVolume })));
  }

  // Library and Import, the main window's two sections; `section` is the one showing.
  function MacSectionTabs(props) {
    var section = props.section || 'Library';
    return h('div', { className: 'bae-m-sections', role: 'tablist' }, ['Library', 'Import'].map(function (s) {
      return h('span', { key: s, role: 'tab', 'aria-selected': s === section, className: cx('bae-m-section-tab', s === section && 'bae-m-section-tab-on') }, s);
    }));
  }

  // The title bar's Activity button, by what the sync queue does (`state`): moving spins, attention shows the warning mark, and paused (a queue held, Paused or Waiting for network, with nothing moving) shows a pause symbol so the hold can be seen. With nothing moving or held there is no button.
  function MacActivityButton(props) {
    // Every state is one button named Activity, as VoiceOver reads it in the app; what it shows inside is drawing only.
    if (props.state === 'moving') return h('button', { type: 'button', className: 'bae-m-iconbtn', title: 'Activity', 'aria-label': 'Activity' }, h('span', { 'aria-hidden': true, style: { display: 'flex' } }, h(MacSpinner, { size: 14 })));
    if (props.state === 'attention') return h('button', { type: 'button', className: 'bae-m-iconbtn', title: 'Activity', 'aria-label': 'Activity' }, h(StatusGlyph, { tone: 'warning', set: 'mac', size: 'large', weight: 'medium' }));
    if (props.state === 'paused') return h(MacIconButton, { icon: 'pause', label: 'Activity' });
    return null;
  }

  // The person's picture, or initials, that opens their menu: Add device…, Invite someone… (or Set up a cloud home…), Playlists, History, Settings…. `name`: their name (the boards' person is Ana); `image`: their picture.
  function MacAvatarButton(props) {
    return h('button', { type: 'button', 'aria-haspopup': 'menu', 'aria-label': 'Your menu', className: 'bae-m-iconbtn bae-m-avatarbtn' }, h(Avatar, { name: props.name || 'Ana', image: props.image, size: 22 }));
  }

  // The main window's title bar: the section tabs in the middle, then the search field, the Activity button (`sync`) and the avatar at the end.
  function MacTitleBar(props) {
    return h('header', { className: 'bae-m-titlebar' },
      h('div', { className: 'bae-m-titlebar-center' }, h(MacSectionTabs, { section: props.section })),
      h('div', { className: 'bae-m-titlebar-trail' },
        h(MacSearchField, { value: props.search, focused: props.searchFocused }),
        h(MacActivityButton, { state: props.sync }),
        h(MacAvatarButton, { name: props.avatar, image: props.avatarImage })));
  }

  // The main window's frame, its own as in the app (MainWindowChrome): the rounded window with the traffic lights, the title bar over the library or Import and the side pane, then the now playing bar.
  function MacMainWindow(props) {
    return h('div', { className: 'bae-m-mainwin', style: { width: Number(props.width || 1200), height: Number(props.height || 760), flex: 'none' } }, macLights(),
      h(MacTitleBar, { section: props.section, search: props.search, searchFocused: props.searchFocused, sync: props.sync, avatar: props.avatar, avatarImage: props.avatarImage }),
      h('div', { className: 'bae-m-main-body' }, h('div', { className: 'bae-m-main-content' }, kids(props.children).filter(function (c) { var p = inner(c); return !(p && p.type === MacQueuePanel); })), kids(props.children).filter(function (c) { var p = inner(c); return p && p.type === MacQueuePanel; })),
      h(MacNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, shuffle: props.shuffle, repeat: props.repeat, casting: props.casting, noVolume: props.noVolume, queueOpen: props.queueOpen }));
  }

  function MacLibraryHeading(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: cx('bae-m-libhead', collapsed && 'bae-m-libhead-collapsed') },
      // The view's name is a menu of the library's views.
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-libhead-title' }, props.view || 'Albums', h('span', { className: 'bae-m-libhead-chevron', 'aria-hidden': true }, h(Glyph, { set: 'mac', name: 'down', size: collapsed ? 'badge' : 'medium' }))),
      h('div', { className: 'bae-m-libhead-trail' }, props.children));
  }

  // As many columns as the width holds tiles of 200 or more, as the app fits them.
  function MacAlbumGrid(props) {
    return h('div', { className: 'bae-m-grid' }, props.children);
  }

  // `unplayable`: only another device holds the album's files, so its cover and words show dimmed; the tile still opens the album. With no `artist`, the tile has no credit line, as under an artist's group when that artist is the album's only credit.
  function MacAlbumTile(props) {
    return h('div', { className: cx('bae-m-tile', props.selected && 'bae-m-tile-selected', props.unplayable && 'bae-tile-unplayable') },
      h('div', { className: 'bae-m-tile-art' },
        h(Artwork, { title: props.title, missing: props.noCover, radius: 'radius-cover', shadow: 'tile', ring: props.open, dimmed: props.unplayable }),
        props.hover ? h('span', { className: 'bae-m-tile-menu' }, h(MacMoreMenu, { overlay: true, label: 'Album options' })) : null),
      h('div', { className: 'bae-m-tile-title' }, props.title),
      props.artist ? h('div', { className: 'bae-m-tile-artist' }, props.artist) : null,
      h('div', { className: 'bae-m-tile-year' }, props.year || ' '));
  }

  // A track row's lead: the track's number; under the pointer (`hover`) a Play button, or Pause on the current track while it plays; on the current track otherwise the speaker, which a screen reader names Now playing.
  // The three stay in the lead's layout, one cell over the other, and only the shown one draws, so the lead, and the row, keep one size in every state, as the app's lead does.
  function MacTrackLead(props) {
    var pause = props.current && props.playing;
    var shown = props.hover ? 'button' : props.current ? 'speaker' : 'number';
    var off = function (state) { return shown !== state && 'bae-m-track-lead-off'; };
    return h('span', { className: 'bae-m-track-lead' },
      h('span', { className: cx('bae-m-track-pos', off('number')) }, props.position),
      h('button', { type: 'button', className: cx('bae-plainbtn', off('button')), 'aria-label': pause ? 'Pause' : 'Play' }, h(Glyph, { set: 'mac', name: pause ? 'pause' : 'play', size: 'medium' })),
      h(Glyph, { set: 'mac', name: 'speaker', size: 'small', className: cx('bae-accent', off('speaker')), label: 'Now playing' }));
  }

  // One track of an album's list. `picked`: among the tracks picked together, on the soft accent, and marked selected, as the app marks the row's texts. `rating`: the row of an album's list, which keeps a slot for the track's stars after the row (0 leaves the slot empty, as the app does for a track nobody rated). `ratingSystem`: the library's system for tracks; `heart` fills the slot with a heart when the stars are three or more, `none` draws no slot.
  function MacTrackRow(props) {
    var row = h('div', { className: cx('bae-m-track', props.picked ? 'bae-m-track-picked' : props.hover && 'bae-m-track-hover', props.artist && 'bae-m-track-two'), 'aria-selected': props.picked ? 'true' : null },
      h(MacTrackLead, { position: props.position, hover: props.hover, current: props.current, playing: props.playing }),
      h('div', { className: 'bae-m-track-text' },
        h('div', { className: cx('bae-m-track-title', props.current && 'bae-accent') }, props.title),
        props.artist ? h('div', { className: 'bae-m-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-m-track-time' }, props.duration));
    // Every album list keeps a slot for the track's stars after the row, empty when you gave it none, so every row's duration lines up.
    if (props.ratingSystem === 'none') return row;
    var n = Number(props.rating || 0), heart = props.ratingSystem === 'heart', shown = heart ? n >= 3 : n > 0;
    return h('div', { className: 'bae-m-track-rated' }, h('div', { className: 'bae-m-track-rated-row' }, row),
      h('span', { className: cx('bae-m-track-stars', !shown && 'bae-m-track-stars-none'), 'aria-hidden': !shown || null }, heart ? h(Heart, { set: 'mac', on: true, size: 10 }) : h(Stars, { value: n, size: 10 })));
  }

  // The eyebrow has no space of its own: each place that shows one spaces it, as the app's places do.
  function MacEyebrow(props) { return h('div', { className: 'bae-m-eyebrow' }, props.children); }

  // `options`: the page's ⋯ button at the right with these actions in its menu, as an album's options button; `optionsOpen` draws the menu open, `highlighted` the item under the pointer.
  function MacDetailHeader(props) {
    return h('div', { className: 'bae-m-dethead' },
      h(Artwork, { title: props.name, size: 72, radius: 'radius-cover', missing: props.noImage }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-m-dethead-name' }, props.name), h('div', { className: 'bae-m-dethead-count' }, props.count)),
      props.options ? h(MacMoreMenu, { label: props.optionsLabel || 'Options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted }) : null);
  }

  // A playlist page's head, laid out as the album page's: the picture beside a column of the name (up to two lines), the owner on a line of their own, the facts line (count and length), then Play and the ⋯ under the text, at its start. The ⋯ menu offers Shuffle to everyone, then the owner's `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer. The picture is as tall as the text column. `sharing`: on the person's own playlist, who sees it, as a lock (Only me) or people (Everyone) glyph at the facts line's end; a playlist someone else shared shows nothing about sharing.
  function MacPlaylistHeader(props) {
    var shared = props.sharing ? h('span', { className: 'bae-m-plpage-sharing', role: 'img', 'aria-label': props.sharing, title: props.sharing }, h(Glyph, { set: 'mac', name: props.sharing === 'Everyone' ? 'people' : 'lock', size: 'small' })) : null;
    return h('div', { className: 'bae-m-plpage-head' },
      h(PlaylistPicture, { covers: props.covers, size: 103, radius: 'radius-cover' }),
      h('div', { className: 'bae-m-plpage-text' },
        h('h1', { className: 'bae-m-plpage-name' }, props.name),
        h(MacPersonTag, { name: props.owner, strong: true }),
        h('div', { className: 'bae-m-plpage-facts' }, [props.count, props.length].join(' · '), shared),
        h('div', { className: 'bae-m-plpage-actions' },
          h(MacButton, { kind: 'primary' }, 'Play'),
          h(MacMoreMenu, { boxed: true, label: 'Playlist options', items: ['Shuffle'].concat(list(props.options)).join(','), open: props.optionsOpen, highlighted: props.highlighted }))));
  }

  // A playlist's page inside the side pane's Playlists tab, whose head shows the back button to the list (`MacQueuePanel`'s `back`): its header (`MacPlaylistHeader`, which takes the same props), then its tracks as children, `MacListRow`s.
  function MacPlaylistPage(props) {
    return h('section', { className: 'bae-m-plpage' },
      h(MacPlaylistHeader, { name: props.name, owner: props.owner, covers: props.covers, count: props.count, length: props.length, sharing: props.sharing, options: props.options, optionsOpen: props.optionsOpen, highlighted: props.highlighted }),
      h('div', { className: 'bae-m-plpage-tracks' }, props.children));
  }

  // A queued track: the playlist page's row, the cover beside the title over the artists, the length at the end; under the pointer (`hover`) the row tints and Remove from queue joins the end.
  function MacQueueRow(props) {
    return h(MacListRow, { cover: 40, title: props.title, sub: props.artist, album: props.album, duration: props.duration, hover: props.hover, remove: 'Remove from queue' });
  }


  // The close control of a docked pane and of an open album: an × in a round, quiet fill.
  function MacCloseButton(props) {
    return h('button', { type: 'button', className: 'bae-m-close', 'aria-label': props.label || 'Close', title: props.label || 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 'medium' }));
  }

  // The choice of an album's release, one segment for each of `releases` (comma-separated), `selected` the one showing; an album of one release has no choice to draw.
  function MacReleasePicker(props) {
    var releases = list(props.releases);
    if (releases.length < 2) return null;
    return h(MacSegmentedControl, { options: releases.join(','), selected: props.selected || releases[0] });
  }

  // The open album's head: the title, the artist, the year, each on its own line, the release picker (`releases`, `release`), the edition line (`edition`) and the copy line (`copy`), with the catalogs' popover when catalogs describe the release (`records`, `recordsHover` as `releaseFactsLine` draws them, and `recordsOpen`, which draws the popover open after the header; the panel sets `recordsAnchor` instead, so the header marks its button open and the panel draws the popover), the line saying why this Mac can't play it (`unplayable`), Play and the album's ⋯ (`options`, `optionsOpen`, `highlighted`, a boxed MacMoreMenu). After the ⋯ comes the person's own rating, read-only (`rating`, "4 ★"): nothing while they have not rated the album. The rest of the album's actions, the rating included, are items of the ⋯ menu.
  // The album's read-only rating after its ⋯ or year: "4 ★" under Stars, one filled heart under Heart at three stars or more, nothing under None or while the person has not rated it (or not hearted it). `set`: the heart's, 'mac' on macOS.
  function albumMark(rating, system, size, className, set) {
    var n = Number(rating || 0);
    if (system === 'none' || !n || (system === 'heart' && n < 3)) return null;
    var mark = system === 'heart' ? h(Heart, { set: set, on: true, size: size }) : h(Stars, { value: n, size: size, number: true });
    return className ? h('span', { className: className }, mark) : mark;
  }

  // The release's edition line and copy line. When catalogs describe the release (`records`, their names, comma-separated), the lines and an ↗ form one button: `recordsHover` draws its hover plate, and `recordsOpen` or `recordsAnchor` makes it the button the popover hangs from.
  function releaseFactsLine(props) {
    var lines = props.edition || props.copy ? h('div', { className: 'bae-m-albumpanel-facts' }, props.edition ? h('div', null, props.edition) : null, props.copy ? h('div', null, props.copy) : null) : null;
    if (!list(props.records).length) return lines;
    return h('div', { className: cx('bae-m-albumfacts-anchor', (props.recordsOpen || props.recordsAnchor) && 'bae-m-albumfacts-anchor-open') },
      // VoiceOver reads the button as its parts with a comma between, the arrow's the word Identified.
      h('button', { type: 'button', 'aria-label': [props.edition, props.copy, 'Identified'].filter(Boolean).join(', '), className: cx('bae-m-albumfacts', props.recordsHover && 'bae-m-albumfacts-hover') }, lines,
        // The arrow is arrow.up.right in the detail text's font (11.5 points, regular).
        h(Glyph, { set: 'mac', name: 'external', size: 11.5, weight: 'regular', className: 'bae-m-albumfacts-arrow' })));
  }
  // The popover of the catalogs' links, 320pt wide, centred under the facts button with its arrow pointing at it. It is a window of its own, so it comes after everything the header or the panel draws.
  function releaseRecordsPopover(records) {
    return h('div', { className: 'bae-m-albumfacts-pop' }, h(MacPopover, { flush: true, width: 320, top: 0, right: 0, arrow: 160 }, h('div', { className: 'bae-m-albumfacts-card' }, h(MacRecordLinks, { records: records }))));
  }

  function MacAlbumHeader(props) {
    var picker = h(MacReleasePicker, { releases: props.releases, selected: props.release });
    var popover = props.recordsOpen && list(props.records).length > 0;
    return h('div', { className: cx('bae-m-albumhead', popover && 'bae-m-albumhead-open') },
      h('h2', { className: 'bae-m-albumpanel-title' }, props.title),
      h('div', { className: 'bae-m-albumpanel-sub' }, h('span', { className: 'bae-muted' }, props.artist), props.year ? [h('span', { key: 'd', className: 'bae-m-tertiary' }, '·'), h('span', { key: 'y', className: 'bae-m-tertiary' }, props.year)] : null),
      list(props.releases).length > 1 ? h('div', { className: 'bae-m-albumpanel-releases' }, picker) : null,
      releaseFactsLine(props),
      // `unplayable`: where the files are, when only another device holds them; Play gives way to the words Not on this device, whose help tag names the device (`unplayableHover` draws it showing).
      h('div', { className: 'bae-m-albumpanel-actions' }, props.unplayable ? h(MacHelpTag, { text: props.unplayableHover ? props.unplayable : null }, h('span', { className: 'bae-m-albumpanel-unplayable' }, 'Not on this device')) : h(MacButton, { kind: 'primary' }, 'Play'), h(MacMoreMenu, { boxed: true, label: 'Album options', items: props.options, open: props.optionsOpen, highlighted: props.highlighted }),
        albumMark(props.rating, props.ratingSystem, 13, 'bae-m-albumpanel-rating', 'mac')),
      popover ? releaseRecordsPopover(props.records) : null);
  }

  // A help tag: the short words macOS shows under the pointer after it rests on a control (SwiftUI's `.help`), drawn under the control it describes; with no `text`, only the control.
  function MacHelpTag(props) {
    return h('span', { className: 'bae-m-helptag-anchor' }, props.children, props.text ? h('span', { role: 'tooltip', className: 'bae-m-helptag' }, props.text) : null);
  }

  // An album's credits: the Credits heading over its role and name pairs (`credits`: "Role: name" pairs separated by semicolons, as `MacFactsGrid` takes them).
  function MacAlbumCredits(props) {
    return h('div', { className: 'bae-m-albumcredits' }, h(MacFormHeading, null, 'Credits'), h(MacFactsGrid, { facts: props.credits, wide: true }));
  }

  // The works an album's tracks record: the Works heading over one line a work, its track's number, the work's title in the accent, a button that opens the work's page, and its composer (`works`: "number|title|composer" entries separated by semicolons).
  function MacAlbumWorks(props) {
    var works = String(props.works || '').split(';').map(function (w) { return w.split('|').map(function (x) { return x.trim(); }); }).filter(function (w) { return w[1]; });
    return h('div', { className: 'bae-m-albumcredits' }, h(MacFormHeading, null, 'Works'),
      h('div', { className: 'bae-m-albumworks' }, works.map(function (w, i) {
        return h('div', { key: i, className: 'bae-m-albumworks-row' }, h('span', { className: 'bae-muted bae-num bae-m-albumworks-pos' }, w[0]), h('button', { type: 'button', className: 'bae-plainbtn bae-accent' }, w[1]), w[2] ? h('span', { className: 'bae-muted' }, '· ' + w[2]) : null);
      })));
  }

  // The album opened in the grid: a panel under the album's row, its 340pt cover beside its header (`MacAlbumHeader`, which takes the same props), then its tracks, credits and works as children, then the release's audio line (`audio`). The catalogs' popover (`recordsOpen`) comes last.
  function MacAlbumPanel(props) {
    return h('section', { className: 'bae-m-albumpanel' },
      h('span', { className: 'bae-m-albumpanel-close' }, h(MacCloseButton)),
      h(Artwork, { title: props.title, size: 340, radius: 'radius-cover', missing: props.noCover, className: 'bae-m-albumpanel-cover' }),
      h('div', { className: 'bae-m-albumpanel-body' },
        h(MacAlbumHeader, { title: props.title, artist: props.artist, year: props.year, releases: props.releases, release: props.release, edition: props.edition, copy: props.copy, records: props.records, recordsHover: props.recordsHover, recordsAnchor: props.recordsOpen, unplayable: props.unplayable, unplayableHover: props.unplayableHover, options: props.options, optionsOpen: props.optionsOpen, highlighted: props.highlighted, rating: props.rating, ratingSystem: props.ratingSystem }),
        h('div', { className: 'bae-m-albumpanel-tracks bae-m-albumpanel-tracks-rated' }, props.children),
        props.audio ? h('div', { className: 'bae-m-albumpanel-audio' }, props.audio) : null),
      props.recordsOpen && list(props.records).length ? releaseRecordsPopover(props.records) : null);
  }

  // A side's tracks in two columns once it has more than eight.
  // `heading`: the side's name ("Side A"), an eyebrow over its tracks.
  function MacTrackColumns(props) {
    var cols = h('div', { className: 'bae-m-trackcols' }, props.children);
    return props.heading ? h(React.Fragment, null, h('div', { className: 'bae-m-trackcols-heading' }, h(MacEyebrow, null, props.heading)), cols) : cols;
  }

  // Artists and composers: the list on the left, the selected one's page on the right.
  function MacBrowseSplit(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-m-split' },
      h('div', { className: 'bae-m-split-list' }, list[0]),
      h('div', { className: 'bae-m-split-detail' }, list.slice(1)));
  }

  // The tile is one button that opens the album, named by its title and year. `unplayable`: dimmed as `MacAlbumTile` draws it.
  function MacArtistAlbumTile(props) {
    return h('button', { type: 'button', className: cx('bae-plainbtn bae-m-atile', props.unplayable && 'bae-tile-unplayable') }, h(Artwork, { title: props.title, radius: 'radius-artwork', missing: props.noCover, dimmed: props.unplayable }),
      h('div', { className: 'bae-m-tile-title' }, props.title), h('div', { className: 'bae-m-atile-year' }, props.year || ' '));
  }


  // An artist page's albums, `MacArtistAlbumTile`s as children, as many to a row as fit.
  function MacArtistAlbumGrid(props) {
    return h('div', { className: 'bae-m-artistgrid' }, props.children);
  }

  // An artist page's other names: the Also credited as label over the field that adds one, with Add, which stays off until the field holds a name (`value`).
  function MacArtistOtherNames(props) {
    return h('div', { className: 'bae-m-othernames' },
      h('div', { className: 'bae-m-othernames-label' }, 'Also credited as'),
      h('div', { className: 'bae-m-othernames-add' }, h(MacTextField, { placeholder: 'Another name', value: props.value }), h(MacButton, { off: !props.value }, 'Add')));
  }

  // One sort criterion: its field's menu, its direction (`direction`: up or down) and, when the sort has several criteria (`removable`), remove.
  function MacSortPill(props) {
    var down = props.direction === 'down';
    return h('span', { className: 'bae-m-sortpill' },
      // The field is a menu of the fields.
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-sortpill-field' }, props.field, h(Glyph, { set: 'mac', name: 'down', size: 'badge', text: true, className: 'bae-muted' })),
      // The arrow, arrow.up or arrow.down in the 8.5 by 11 pt frame the app sets, flips it.
      h(Glyph, { set: 'mac', name: down ? 'arrowDown' : 'arrowUp', size: 'small', frame: [8.5, 11], button: down ? 'Descending' : 'Ascending', className: 'bae-muted' }),
      props.removable ? h('button', { type: 'button', className: 'bae-plainbtn bae-muted', 'aria-label': 'Remove sort criterion' }, h(Glyph, { set: 'mac', name: 'close', size: 'badge' })) : null);
  }

  // The sort's criteria (`criteria`: "Field:down,Field:up"), each a `MacSortPill`; then + to add one, while a field of the list (`fields`, the albums' Title, Artist, Year and Date added unless given) is still unused.
  function MacSortCriteria(props) {
    var items = String(props.criteria || 'Date added:down').split(',').map(function (c) { var p = c.trim().split(':'); return { field: p[0], dir: p[1] || 'up' }; });
    return h('div', { className: 'bae-m-sortrow' },
      items.map(function (it, i) { return h(MacSortPill, { key: i, field: it.field, direction: it.dir, removable: items.length > 1 }); }),
      list(props.fields || 'Title, Artist, Year, Date added').every(function (f) { return items.some(function (it) { return it.field === f; }); }) ? null : h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-plainbtn bae-m-sortadd', 'aria-label': 'Add sort criterion' }, h(Glyph, { set: 'mac', name: 'add', size: 'medium' })));
  }

  // A checkbox and its label, as wide as its content, as AppKit sizes a checkbox. `detail`: the label in the detail type, as under an alert's message. The check lies over the box, as the app overlays it, so a checked box lines up with the row as an empty one does.
  function MacCheckbox(props) {
    return h('label', { className: cx('bae-m-checkbox', props.detail && 'bae-m-checkbox-detail'), role: 'checkbox', 'aria-checked': !!props.on }, h('span', { className: cx('bae-m-checkbox-box', props.on && 'bae-m-checkbox-on') }, props.on ? h('span', { className: 'bae-m-checkbox-mark' }, h(Glyph, { set: 'mac', name: 'checkmark', size: 'chip' })) : null), props.label);
  }

  // Heads an artist's albums when the grid is grouped by artist: the artist's picture, as the artist list draws it, beside the name over the album count. `noImage`: the artist has no picture.
  function MacArtistGroupHeading(props) {
    return h('div', { className: 'bae-m-groupheading' },
      h(Artwork, { title: props.name, size: 'size-row-artwork', radius: 'radius-artwork', missing: props.noImage }),
      h('div', { className: 'bae-m-groupheading-text' }, h('h2', { className: 'bae-m-groupheading-name' }, props.name), h('div', { className: 'bae-m-groupheading-count' }, props.count)));
  }

  // A Material menu: `AndroidMenuItem`s, a "Sort by" style label, dividers.
  function AndroidMenu(props) {
    // A menu sizes to its items; `width` is a minimum only a board that needs one passes. `top` defaults to 56, under a top bar; a `top` of 0 stays 0.
    var style = { position: 'absolute', top: props.top != null ? Number(props.top) : 56 };
    if (props.width != null) style.minWidth = Number(props.width);
    if (props.left != null) style.left = Number(props.left); else style.right = Number(props.right != null ? props.right : 8);
    return h('div', { className: 'bae-a-menu', role: 'menu', style: style }, props.children);
  }
  function AndroidMenuItem(props) {
    if (props.divider) return h('div', { className: 'bae-a-menu-divider', role: 'separator' });
    if (props.heading) return h('div', { className: 'bae-a-menu-heading' }, props.children);
    // `label`: the item's name for a screen reader when it shows a picture in place of words, such as a row of stars.
    return h('div', { className: cx('bae-a-menu-item', props.selected && 'bae-a-menu-item-on', props.pressed && 'bae-a-menu-item-pressed'), role: 'menuitem', 'aria-label': props.label },
      h('span', null, props.children),
      props.check != null ? h('span', { className: 'bae-a-menu-check', style: { opacity: props.check ? 1 : 0 } }, h(Material, { name: 'check', size: 'large' })) : null);
  }

  // ---- Android library ----

  function AndroidPlayPause(props) {
    // The button is the glyph and 8dp around it, which the waiting arc rings.
    var icon = props.size || 'large', box = 'calc(' + iconSize(icon, true) + ' + 16px)';
    return h('button', { type: 'button', className: 'bae-a-playpause', style: { width: box, height: box }, 'aria-label': props.playing || props.waiting ? 'Pause' : 'Play' },
      props.waiting ? h('svg', { className: 'bae-waitarc', style: { width: box, height: box }, viewBox: '0 0 48 48', 'aria-hidden': true }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { name: props.playing || props.waiting ? 'pause' : 'play', size: icon }));
  }

  // The playing track on Android: its cover beside its title and artist.
  function AndroidNowPlayingTrack(props) {
    // One button that opens the full player, named as TalkBack reads the merged track.
    return h('button', { type: 'button', className: 'bae-a-np-info', 'aria-label': props.artist ? 'Now playing, ' + props.title + ' by ' + props.artist : props.title }, h(Artwork, { title: props.album || props.title, size: 'size-android-bar-artwork', radius: 'radius-artwork', missing: props.noCover }),
      h('div', { className: 'bae-a-np-text', 'aria-hidden': true }, h('div', { className: 'bae-a-np-title' }, props.title), h('div', { className: 'bae-a-np-sub' }, props.artist)));
  }

  // Previous, play and pause, next. `large` is the full player's: bigger buttons, spread across the sheet; `waiting` draws the arc around play while the track waits on storage.
  function AndroidTransport(props) {
    var large = !!props.large;
    return h('div', { className: cx('bae-a-transport', large && 'bae-a-transport-large') },
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 'androidTransport' : null, icon: 'previous', label: 'Previous' }),
      h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting, size: large ? 'androidTransportPlay' : null }),
      h(AndroidIconButton, { box: large ? 56 : 40, size: large ? 'androidTransport' : null, icon: 'next', label: 'Next' }));
  }

  function AndroidNowPlayingBar(props) {
    if (!props.title && !props.error) return null;
    return h('div', { className: 'bae-a-npbar' },
      h('div', { className: 'bae-a-np-row' },
        props.error ? h('div', { className: 'bae-a-np-info bae-a-np-error' }, props.error) : h(AndroidNowPlayingTrack, { title: props.title, artist: props.artist, album: props.album, noCover: props.noCover }),
        h(AndroidTransport, { playing: props.playing, waiting: props.waiting }),
        props.castOff ? null : h(AndroidIconButton, { box: 40, icon: 'cast', label: 'Play on…', tone: props.casting ? 'accent' : 'muted' }),
        h(AndroidIconButton, { box: 40, icon: 'queue', label: 'Queue' }),
        h(AndroidIconButton, { box: 40, icon: 'repeat', label: 'Repeat', tone: props.repeat ? 'accent' : 'muted' })),
      h(AndroidSeekBar, { held: props.held, elapsed: props.elapsed, duration: props.duration }));
  }

  // The library header while searching: Close search, and the search field, an editable text field holding what was typed (`search`) with its Clear search button. The field is the whole pill, as AndroidSearchBar.kt's BasicTextField takes its decoration box, and holds Clear search. While the field is empty the prompt shows as text under it, as the app draws its placeholder Text behind the field, and the prompt also names the field, as Android names an empty field by the text its decoration box draws.
  function AndroidSearchBar(props) {
    const prompt = 'Search albums, tracks, composers, and works';
    return h('header', { className: 'bae-a-searchbar' }, h(AndroidIconButton, { icon: 'back', label: 'Close search' }),
      h('div', { className: 'bae-a-searchfield' },
        props.search ? null : h('span', { className: 'bae-a-search-prompt' }, prompt),
        h('input', { className: cx('bae-a-searchinput', props.search && 'bae-a-searchinput-clearable'), type: 'text', defaultValue: props.search || '', spellCheck: false, 'aria-label': props.search ? null : prompt }),
        props.search ? h(AndroidInlineIconButton, { icon: 'close', label: 'Clear search' }) : null));
  }

  // The Activity button, by what the sync queue does (`state`): moving spins, attention shows the warning mark, and paused (a queue held with nothing moving) shows a pause symbol so the hold can be seen. With nothing moving or held there is no button. Every state is one button named Activity, as Activity.kt's AndroidIconButton is; the moving spinner inside it stays a progress indicator, as TalkBack reads it.
  function AndroidActivityButton(props) {
    if (props.state === 'moving') return h('button', { type: 'button', className: 'bae-a-iconbtn', 'aria-label': 'Activity' }, h(AndroidSpinner, { size: 16 }));
    if (props.state === 'attention') return h(AndroidIconButton, { icon: 'warning', label: 'Activity', tone: 'warning' });
    if (props.state === 'paused') return h(AndroidIconButton, { icon: 'pause', label: 'Activity' });
    return null;
  }

  // The library's header: the view's title, which opens the view menu, then search, the Activity button (`sync`), ⋮ and the avatar; `collapsed` once the grid scrolls; `searching` swaps it for `AndroidSearchBar`.
  function AndroidLibraryHeader(props) {
    var collapsed = !!props.collapsed;
    if (props.searching) return h(AndroidSearchBar, { search: props.search });
    return h('header', { className: cx('bae-a-libheader', collapsed && 'bae-a-libheader-collapsed') },
      h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-a-libhead-title' }, h('span', null, props.view || 'Albums'), h(Glyph, { name: 'down', size: collapsed ? 'small' : 'medium', className: 'bae-a-libhead-chevron' })),
      h(AndroidIconButton, { icon: 'search', label: 'Search' }),
      h(AndroidActivityButton, { state: props.sync }),
      h(AndroidIconButton, { icon: 'more', label: 'More options' }),
      // The person's picture, or initials, ends the bar and opens their menu: Add device, Invite someone, Playlists, History, Settings. `avatar`: their name (the boards' person is Ana); `avatarImage`: their picture.
      h('button', { type: 'button', 'aria-haspopup': 'menu', 'aria-label': 'Your menu', className: 'bae-a-iconbtn' }, h(Avatar, { name: props.avatar || 'Ana', image: props.avatarImage, size: 32, android: true })));
  }

  function AndroidLibraryScreen(props) {
    var split = splitOverlays(props.children);
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h(AndroidLibraryHeader, { view: props.view, collapsed: props.collapsed, searching: props.searching, search: props.search, sync: props.sync, avatar: props.avatar, avatarImage: props.avatarImage }),
      h('main', { className: 'bae-a-libbody' }, split.content),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat, error: props.barError }),
      split.over);
  }

  // A page about one thing under its back bar. The page's content carries its own padding, as each page's list does in the app (the album's in `AndroidAlbumDetailContent`).
  function AndroidDetailScreen(props) {
    var split = splitOverlays(props.children);
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h(AndroidTopBar, { backLabel: props.back ? 'Back to ' + props.back : 'Back to library', more: props.menu ? (props.menuLabel || 'Album options') : null }),
      h('main', { className: 'bae-a-libbody' }, split.content),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, noCover: props.trackNoCover, playing: props.playing, waiting: props.waiting, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }),
      split.over);
  }

  // The album page below its bar: its blocks (the header, the release's controls, the tracks, the credits) in a 16dp column, 8dp apart.
  function AndroidAlbumDetailContent(props) {
    return h('div', { className: 'bae-a-albumcontent' }, props.children);
  }

  function AndroidAlbumGrid(props) {
    return h('div', { className: 'bae-a-grid' }, props.children);
  }

  // An album's tile: one button that opens the album, named by its title and line, as TalkBack reads the merged tile. `unplayable`: only another device holds the album's files, so its cover and words show dimmed; the tile still opens the album.
  function AndroidAlbumTile(props) {
    return h('button', { type: 'button', className: cx('bae-plainbtn', 'bae-a-tile', props.unplayable && 'bae-tile-unplayable') },
      h(Artwork, { title: props.title, missing: props.noCover, radius: 'radius-cover', dimmed: props.unplayable }),
      h('div', { className: 'bae-a-tile-title' }, props.title),
      h('div', { className: 'bae-a-tile-line' }, props.line));
  }

  function AndroidArtistGroupHeading(props) {
    if (props.pinned) return h('div', { className: 'bae-a-artisthead-pinned' }, h(Artwork, { title: props.name, size: 32, round: true, missing: props.noImage }), h('div', { className: 'bae-a-artisthead-name' }, props.name));
    return h('div', { className: 'bae-a-artisthead' }, h(Artwork, { title: props.name, size: 'size-android-row-artwork', round: true, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-a-artisthead-name' }, props.name), h('div', { className: 'bae-a-artisthead-count' }, props.count)));
  }

  // A track row's lead on Android: the track's number, or on the current track Play, or the playing mark while it plays. A screen reader names the playing mark Now playing and the Play mark Paused.
  function AndroidTrackLead(props) {
    return h('span', { className: 'bae-a-track-lead' }, props.current ? h(Glyph, { name: props.playing ? 'nowplaying' : 'play', size: 'large', className: 'bae-accent', label: props.playing ? 'Now playing' : 'Paused' }) : h('span', { className: 'bae-a-track-pos' }, props.position));
  }

  // `work`: the work the track records, with its composer, on a line under the row.
  function AndroidTrackRow(props) {
    var row = h('div', { className: 'bae-a-track' },
      h(AndroidTrackLead, { position: props.position, current: props.current, playing: props.playing }),
      h('div', { className: 'bae-a-track-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), props.artist ? h('div', { className: 'bae-a-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-a-track-time' }, props.duration),
      h(AndroidIconButton, { icon: 'more', label: 'Track options', tone: 'muted' }));
    return props.work ? h('div', null, row, h('div', { className: 'bae-a-track-work' }, props.work)) : row;
  }

  function AndroidEyebrow(props) { return h('div', { className: 'bae-a-eyebrow' }, props.children); }

  function AndroidAlbumHeader(props) {
    var mark = albumMark(props.rating, props.ratingSystem, 12, 'bae-a-albumhead-mark');
    return h('div', { className: 'bae-a-albumhead' }, h(Artwork, { title: props.title, size: 150, radius: 'radius-cover', missing: props.noCover }),
      h('div', { className: 'bae-a-albumhead-text' }, h('div', { className: 'bae-a-albumhead-title' }, props.title), h('div', { className: 'bae-a-albumhead-artist' }, props.artist), props.year || mark ? h('div', { className: 'bae-a-albumhead-fact' }, props.year, props.year && mark ? ' · ' : null, mark) : null,
        props.edition ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-release bae-a-albumhead-gap' }, props.edition) : null, props.copy ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-release' }, props.copy) : null,
        // `unplayable`: only another device holds the files, so the words Not on this device stand where Play stood, and the info button opens a sheet that names the device.
        props.unplayable ? h('div', { className: 'bae-a-albumhead-actions' }, h('span', { className: 'bae-a-albumhead-unplayable' }, 'Not on this device'), h(AndroidIconButton, { icon: 'info', label: 'Show where it is', size: 'medium', tone: 'muted' })) : null,
        props.children ? h('div', { className: 'bae-a-albumhead-actions' }, props.children) : null));
  }

  // The album page's pin control for one release while a pin is under way or has failed, as AndroidDownloadControlContainer draws it: Queued to pin, with Cancel; Pinning with its bar and bytes and Cancel pinning; Could not pin with Details and Retry, and Cancel. Pin and Unpin are items of the album's ⋮ menu. Nothing says how many wait ahead or what holds the queue back.
  function AndroidPinControl(props) {
    var s = props.state || 'queued';
    function btn(text) { return h(AndroidButton, { kind: 'outlined' }, text); }
    var line = function (t) { return h('span', { className: 'bae-a-fineline' }, t); };
    var body =
      s === 'downloading' ? [h(AndroidProgress, { key: 'p', name: 'Pinning', value: Number(props.progress || 0.4) }), h('span', { key: 'b', className: 'bae-a-fine' }, 'Pinning · ' + (props.bytes || '120 MB of 300 MB')), h('div', { key: 'c' }, btn('Cancel pinning'))] :
      s === 'failed' ? [h(AndroidNotice, { key: 'e', quiet: true, tone: 'danger', title: 'Could not pin', secondary: 'Details', action: 'Retry' }), h('div', { key: 'r' }, btn('Cancel'))] :
      h('div', { className: 'bae-a-dlrow' }, line('Queued to pin'), btn('Cancel'));
    return h('div', { className: 'bae-a-dl' }, body);
  }

  // The album page's upload line, as ReleaseUploadLine draws it: the phase core words, Queued to upload while the files wait; a missing original (`missing`) as a quiet warning notice naming it.
  function AndroidUploadLine(props) {
    return h('div', { className: 'bae-a-dl' }, h('span', { className: 'bae-a-fineline' }, props.phase || 'Queued to upload'),
      props.missing ? h(AndroidNotice, { quiet: true, tone: 'warning', title: props.missing + ' is missing', subtitle: 'Restore it to finish uploading.' }) : null);
  }
