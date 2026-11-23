  // ---- Search ----
  // Search's results, in a card hanging under the field: the children, or with `empty` (the search found nothing) No matches.
  function MacSearchCard(props) {
    var style = { position: 'absolute', top: Number(props.top != null ? props.top : 60), right: Number(props.right != null ? props.right : 56), width: 440, zIndex: 6 };
    return h('div', { className: 'bae-m-searchcard', style: style }, props.empty ? h(MacContentUnavailable, { title: 'No matches' }) : props.children);
  }
  function MacSearchSection(props) { return h('div', { className: 'bae-m-searchsection' }, props.title); }
  // One row of a library list on the Mac: Search's results, the artist and composer lists, the composer and work pages' rows.
  // A 44pt cover, the title over a second line 2pt below (it holds its height when empty), and `trailing` (a track's duration). `hover` and `selected` fill it;
  // `loading` is the row before it arrives. `credit` is the text-only row of a credit or a recording, which opens nothing. `bleed` lets the fill reach 8pt past
  // the text, for a row on a page rather than in a list; `part` indents it 16pt under its group's heading work; `top` is the gap above it.
  function MacLibraryRow(props) {
    var ml = (props.bleed && !props.credit ? -8 : 0) + (props.part ? 16 : 0);
    var style = { marginTop: props.top ? Number(props.top) : null, marginLeft: ml || null };
    // A row that opens something is one button, named as macOS names it: its title, second line and trailing words joined with commas; the row before it arrives is that button, off. A credit opens nothing, so its words stay plain text.
    if (props.loading) return h('div', { className: 'bae-m-librow', style: style, role: 'button', 'aria-disabled': true }, h(MacPlaceholderLine, { width: 44, height: 44, radius: 'radius-artwork', fixed: true }),
      h('div', { className: 'bae-m-librow-text' }, h(MacPlaceholderLine, { width: 140, height: 11 }), h(MacPlaceholderLine, { width: 80, height: 10 })));
    // `dropping`: tracks dragged over the row land in it when let go (a playlist in the side pane).
    var button = props.credit ? null : { role: 'button', 'aria-label': [props.title, props.subtitle, props.trailing].filter(Boolean).join(', ') };
    return h('div', Object.assign({ className: cx('bae-m-librow', props.dropping && 'bae-m-librow-drop', props.credit && 'bae-m-librow-credit', props.bleed && !props.credit && 'bae-m-librow-bleed', props.hover && 'bae-m-librow-hover', props.selected && 'bae-m-librow-selected'), style: style }, button),
      props.credit ? null : props.covers != null ? h(PlaylistPicture, { covers: props.covers, size: 'size-row-artwork' }) : h(Artwork, { title: props.art || props.title, size: 'size-row-artwork', radius: 'radius-artwork', missing: props.missing }),
      h('div', { className: 'bae-m-librow-text' }, h('div', { className: 'bae-m-librow-title' }, props.title), h('div', { className: 'bae-m-librow-sub' }, props.subtitle || ' ')),
      props.trailing ? h('span', { className: 'bae-m-librow-trail' }, props.trailing) : null);
  }
  function AndroidSearchSection(props) { return h('div', { className: 'bae-a-searchsection' }, props.title); }
  // One row of a library list: Search's results, the artist and composer lists, the composer and work pages' rows.
  function AndroidLibraryRow(props) {
    return h('div', { className: cx('bae-a-librow', props.part && 'bae-a-librow-part', props.inset && 'bae-a-librow-inset') },
      props.noArt ? null : props.covers != null ? h(PlaylistPicture, { covers: props.covers, size: 'size-android-row-artwork' }) : h(Artwork, { title: props.art || props.title, size: 'size-android-row-artwork', radius: 'radius-artwork', missing: props.missing }),
      h('div', { className: 'bae-a-librow-text' }, h('div', { className: 'bae-a-librow-title' }, props.title), props.subtitle ? h('div', { className: 'bae-a-librow-sub' }, props.subtitle) : null),
      props.trailing ? h('span', { className: 'bae-a-librow-trail' }, props.trailing) : null);
  }

  // ---- Queue and playback (macOS) ----
  // The side pane: Queue, Playlists or History (`tab`), switched by its tabs. Playlists lists the playlists, each a place to drop tracks, or shows the one open (`MacPlaylistPage` as its child); History lists plays, newest first, filtered by whose. Only the queue shows what's playing.
  function MacQueuePanel(props) {
    var tab = props.tab || 'Queue';
    var head = h(MacSidePaneHeader, { tab: tab, back: props.back });
    // `hang`: the width of a leading column the rows hang into the margin (History's times), which the body leaves room for, so the headings and the rows' covers share one edge.
    // `empty` on History: no plays from whoever `whose` names (Everyone unless given), with the whose choice under the line to pick someone else.
    if (tab === 'History' && props.empty) {
      var whose = props.whose || 'Everyone';
      return h('aside', { className: 'bae-m-queue' }, head, h(MacContentUnavailable, { title: whose === 'Everyone' ? 'No plays yet' : 'No plays from ' + whose }, wholeListChoice(whose, true)));
    }
    if (tab !== 'Queue') return h('aside', { className: 'bae-m-queue' }, head, h('div', { className: 'bae-m-queue-body', style: props.hang ? { paddingLeft: 'calc(var(--space-related) + ' + Number(props.hang) + 'px + var(--space-related) + var(--space-line))' } : null }, props.children));
    return h('aside', { className: 'bae-m-queue' },
      head,
      props.title ? h('div', { className: 'bae-m-queue-np' },
        // The playing track, the queue's own: its cover lined up with the text's top, Now playing over the title and artist, then the position strip.
        // VoiceOver reads the card as the app does: Now playing and the title, then Play or Pause at the top right, then the artist, then the position strip.
        h(Artwork, { title: props.album || props.title, size: 'size-bar-artwork', radius: 'radius-artwork', shadow: 'bar', missing: props.noCover }),
        h('div', { className: 'bae-m-np-text' },
          h('div', { className: 'bae-m-nptrack-eyebrow' }, 'Now playing'),
          h('div', { className: 'bae-m-np-title' }, props.title)),
        h('button', { type: 'button', className: 'bae-plainbtn bae-m-queue-np-play', 'aria-label': props.playing ? 'Pause' : 'Play' }, h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 'medium' })),
        h('div', { className: 'bae-m-np-sub bae-m-queue-np-sub' }, props.artist),
        h('div', { className: 'bae-m-queue-strip' }, h('div', { className: 'bae-m-queue-strip-bar', role: 'progressbar', 'aria-label': 'Playback position', 'aria-valuenow': Math.round(Number(props.position || 0) * 100) }, h('div', { style: { width: (Number(props.position || 0) * 100) + '%' } })), h('span', null, props.elapsed || '')),
        // A click anywhere else on the card goes to what is playing: a button under the card's content, as wide and tall as the card. `goOff` turns it off while the track has no album to go to, as when the library no longer holds it.
        h('button', { type: 'button', className: 'bae-plainbtn bae-m-queue-np-go', 'aria-label': 'Go to now playing', disabled: !!props.goOff })) : null,
      props.empty ? h(MacContentUnavailable, { title: 'Queue is empty', description: 'Drag tracks here or play an album' })
        : h('div', { className: 'bae-m-queue-body' }, props.children));
  }
  // The side pane's head: the tabs (Queue, Playlists, History; `tab` is the one shown) centred, close at the end; on Playlists, + at the start makes a new playlist, and while a playlist is open (`back`, where the button goes back to) the back button takes its place.
  function MacSidePaneHeader(props) {
    var tab = props.tab || 'Queue';
    return h('div', { className: 'bae-m-queue-head bae-m-pane-head' },
      tab === 'Playlists' ? h('span', { className: 'bae-m-pane-new' }, props.back ? h(MacBackButton, { to: props.back }) : h(MacIconButton, { icon: 'add', label: 'New playlist…' })) : h('span'),
      h(MacSegmentedControl, { options: 'Queue,Playlists,History', selected: tab }), h('span', { className: 'bae-m-pane-close' }, h(MacCloseButton)));
  }
  // `wide`: more room at the end, for a list whose rows end in a person (History).
  function MacQueueSection(props) {
    // A queue lane's heading, in line with its rows' content, ends in `clear` and `shuffle`. `day`: a day of History, whose rows hang their times in the margin and span the pane: the heading starts at the rows' covers, ends at the pane's edge, and may end in `menu`.
    return h('div', { className: cx('bae-m-queue-section', props.day && 'bae-m-queue-section-day') },
      h('span', { className: 'bae-m-queue-section-title' }, props.title),
      props.clear ? h('button', { type: 'button', className: 'bae-plainbtn bae-m-queue-clear', 'aria-label': props.clear }, 'Clear') : null,
      // `menu`: a choice that applies to the whole list (History's whose plays), at the heading's end, opening a menu of its choices.
      props.menu ? wholeListChoice(props.menu) : null,
      props.shuffle != null ? h(MacIconButton, { icon: 'shuffle', size: 'small', active: props.shuffle, label: props.shuffle ? 'Turn off shuffle' : 'Shuffle' }) : null);
  }
  // A choice that applies to a whole list (History's whose plays), shown as its current value in the accent with a ⌄, opening a menu of the choices. `big`: under an empty list's line, in body text, rather than a heading's small text.
  function wholeListChoice(value, big) {
    return h('button', { type: 'button', 'aria-haspopup': 'menu', className: cx('bae-m-queue-menu', big && 'bae-m-queue-menu-action') }, value, h(Glyph, { set: 'mac', name: 'down', size: big ? 'small' : 'badge' }));
  }
  // A queued track still loading: the queue row's shape, a 40pt cover and two lines, in placeholder grey.
  function MacQueuePlaceholderRow() {
    return h('div', { className: 'bae-m-lrow' }, h(MacPlaceholderLine, { width: 40, height: 40, radius: 'radius-artwork', fixed: true }),
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 'calc(var(--space-inline) + var(--space-hairline))' } }, h(MacPlaceholderLine, { width: 140, height: 12 }), h(MacPlaceholderLine, { width: 90, height: 10 })));
  }
  function MacQueueInsertLine() { return h('div', { className: 'bae-m-queue-insert' }); }

  function MacSidePause(props) {
    var disc = props.kind === 'disc';
    return h('div', { className: 'bae-m-scrim' }, h('div', { className: 'bae-m-face-sheet bae-m-sidepause', role: 'dialog' },
      h('div', { className: 'bae-m-sidepause-title' }, (disc ? 'Disc ' : 'Side ') + (props.label || (disc ? '1' : 'A')) + ' ended'),
      h('div', { className: 'bae-muted' }, 'Playback is paused. Press Play to continue.'),
      props.countdown ? h('div', { className: 'bae-m-sidepause-count' }, 'The next ' + (disc ? 'disc' : 'side') + ' starts in ' + props.countdown + ' seconds.') : null,
      h(MacCheckbox, { label: disc ? 'Pause between discs' : 'Pause between sides', on: !props.unchecked }),
      props.unchecked ? h('div', { className: 'bae-muted' }, 'You can turn this back on in Settings › Playback.') : null,
      h('div', { className: 'bae-m-sidepause-buttons' }, h(MacButton, null, 'Close'), h(MacButton, { kind: 'primary' }, 'Play'))));
  }

  // A receiver, wherever bae lists them (the Play on popover, Settings → Playback): its kind's icon, its name, then its kind, or Playing in the accent for the one playing, which ends in Disconnect in the accent. Clicking any other plays on it. One row per receiver. `menu` (Settings → Playback) puts a ⋯ at the end of every row in place of Disconnect: Details… and, on the one playing, Disconnect.
  // The icon, the spinner, Disconnect and the ⋯ sit at the middle of the row's two lines.
  // `waiting`: the words of a wait on this receiver (Connecting…, or Accept on <the receiver>…), in italic in place of its kind, with a small spinner at the row's end, before the ⋯ in Settings → Playback.
  var CAST_KIND = { cast: 'Chromecast', airplay: 'AirPlay', upnp: 'UPnP' };
  function MacCastRow(props) {
    var glyph = { cast: 'cast', airplay: 'airplay', upnp: 'tv' }[props.kind || 'cast'];
    // A receiver the person clicks to play on is one button, named as macOS names it: its name and its second line (its kind, or the wait) joined with a comma.
    var tap = !props.active && !props.menu;
    // In a row that is one button, the button's name already says the wait, so VoiceOver reads no spinner of its own, as in the app.
    var spinner = props.waiting ? (tap ? h('span', { key: 's', 'aria-hidden': true, className: 'bae-m-castrow-spin' }, h(MacSpinner, { size: 14 })) : h(MacSpinner, { key: 's', size: 14 })) : null;
    var control = props.menu ? h(MacMoreMenu, { key: 'm' }) : props.active ? h(MacButton, { key: 'd', kind: 'borderless', size: 'small' }, 'Disconnect') : null;
    return h('div', Object.assign({ className: 'bae-m-castrow' }, tap ? { role: 'button', 'aria-label': props.name + ', ' + (props.waiting || CAST_KIND[props.kind || 'cast']) } : null), h(MacItem, { wide: true, centerLead: true, centerTrail: true, lead: h(Glyph, { set: 'mac', name: glyph, size: 'medium', className: props.active ? 'bae-accent' : 'bae-muted' }), trail: [spinner, control] },
      h('span', { className: 'bae-m-castpop-text' }, h('span', { className: 'bae-m-castpop-name' }, props.name), props.waiting ? h('span', { className: 'bae-m-castpop-kind bae-m-castpop-waiting' }, props.waiting) : h('span', { className: props.active ? 'bae-m-castpop-state' : 'bae-m-castpop-kind' }, props.active ? 'Playing' : CAST_KIND[props.kind || 'cast']))));
  }
  // The Play on button's popover: the header Play on, then the receivers (`devices`, "Name:kind" entries, `casting` the one playing), or No receivers found when there are none. While bae connects to one (`connecting`, the receiver's name), that receiver's row says Connecting…, or Accept on <the receiver>… once the receiver holds the connection for its owner (`ownerDeciding`).
  function MacCastPopover(props) {
    var devices = String(props.devices || '').split(',').filter(Boolean).map(function (d) { var p = d.trim().split(':'); return { name: p[0], kind: p[1] || 'cast' }; });
    var waiting = props.ownerDeciding ? 'Accept on ' + props.connecting + '…' : 'Connecting…';
    // A MacPopover above the Play on button, its arrow at the middle of its lower edge pointing down at the button; the content pads itself.
    return h(MacPopover, { width: 260, right: props.right != null ? props.right : 120, bottom: props.bottom != null ? props.bottom : 92, arrow: 130, flush: true },
      h('div', { className: 'bae-m-castpop' },
        // The popover's header, Play on, on the rows' inset.
        h('div', { className: 'bae-m-castpop-head' }, h(MacEyebrow, null, 'Play on')),
        devices.length ? devices.map(function (d, i) { return h(MacCastRow, { key: i, name: d.name, kind: d.kind, active: d.name === props.casting, waiting: d.name === props.connecting ? waiting : null }); })
          : h('div', { className: 'bae-m-castpop-none' }, 'No receivers found')));
  }

  // ---- Queue and playback (Android) ----
  function AndroidSheet(props) {
    return h('div', { className: 'bae-a-scrim' }, h('div', { className: 'bae-a-sheet', style: props.top != null ? { top: Number(props.top) } : null }, h('div', { className: 'bae-a-sheet-handle', role: 'button', 'aria-label': 'Drag handle' }), props.title ? h('h2', { className: 'bae-a-sheet-title' }, props.title) : null, props.note ? h('p', { className: 'bae-a-sheet-note' }, props.note) : null, props.children ? h('div', { className: 'bae-a-sheet-body' }, props.children) : null));
  }
  // A queue lane's heading; `clear` names its Clear button for the lane it clears (Clear up next, Clear playing from), as MacQueueSection's.
  function AndroidQueueSection(props) {
    return h('div', { className: 'bae-a-queuelabel' }, h('span', { className: 'bae-a-eyebrow', style: { paddingTop: 0, flex: 1 } }, props.title),
      props.clear ? h(AndroidButton, { kind: 'text', name: props.clear }, 'Clear') : null,
      props.shuffle != null ? h(AndroidIconButton, { icon: 'shuffle', label: props.shuffle ? 'Turn off shuffle' : 'Shuffle', size: 'medium', tone: props.shuffle ? 'accent' : 'muted' }) : null);
  }
  function AndroidQueueRow(props) {
    return h('div', { className: cx('bae-a-qrow', props.dragging && 'bae-a-qrow-drag') }, h(Artwork, { title: props.album || props.title, size: 'size-android-row-artwork', radius: 'radius-artwork' }),
      h('div', { className: 'bae-a-qrow-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), h('div', { className: 'bae-a-track-artist' }, props.artist), props.current ? null : h('div', { className: 'bae-a-track-artist' }, props.album)),
      props.current ? null : [h('span', { key: 'd', className: 'bae-a-track-time', style: { padding: '0 var(--space-related)' } }, props.duration), h(AndroidIconButton, { key: 'x', icon: 'close', label: 'Remove', tone: 'muted' }), h(Glyph, { key: 'h', name: 'drag', size: 'medium', className: 'bae-muted' })]);
  }
  // A grey bar where text or a picture will be while it loads, as MacPlaceholderLine: `width` and `height` in dp, `radius` for a picture's corners (a bar's corners otherwise), `fixed` keeps its width in a row.
  function AndroidPlaceholderLine(props) { return placeholderLine(props); }
  // A queued track not loaded yet: the cover's tile and a bar for each of the row's three lines.
  function AndroidQueuePlaceholderRow() {
    return h('div', { className: 'bae-a-qrow' }, h(AndroidPlaceholderLine, { width: 48, height: 48, radius: 'radius-artwork', fixed: true }),
      h('div', { className: 'bae-a-qrow-text', style: { display: 'flex', flexDirection: 'column', gap: 'var(--space-compact)' } }, h(AndroidPlaceholderLine, { width: 160, height: 12 }), h(AndroidPlaceholderLine, { width: 100, height: 10 }), h(AndroidPlaceholderLine, { width: 120, height: 10 })));
  }
  function AndroidFullPlayer(props) {
    return h('div', { className: 'bae-a-fullplayer' },
      h(AndroidIconButton, { icon: 'down', label: 'Collapse' }),
      h(Artwork, { title: props.album || props.title, radius: 'radius-cover', missing: props.noCover }),
      h('div', { className: 'bae-a-fp-title' }, props.title), h('div', { className: 'bae-a-fp-artist' }, props.artist),
      h(AndroidSeekBar, { large: true, held: props.held, elapsed: props.elapsed, duration: props.duration }),
      h(AndroidTransport, { large: true, playing: props.playing, waiting: props.waiting }),
      h('div', { className: 'bae-a-fp-repeat' }, h(AndroidIconButton, { icon: 'repeat', label: 'Repeat mode', tone: props.repeat ? 'accent' : 'muted' })),
      // `noVolume`, as the Mac bar's: a receiver that takes no volume from bae shows where its volume is set in place of the slider.
      props.noVolume ? h('div', { className: 'bae-a-fp-volume bae-a-fp-novolume' }, 'Volume is set on ' + props.noVolume)
        : h('div', { className: 'bae-a-fp-volume' }, h(AndroidIconButton, { icon: 'nowplaying', label: 'Mute', tone: 'muted' }), h(AndroidVolumeSlider, { value: props.volume })));
  }
  // The full player's volume bar, drawn as its seek line is: the level (`value`, 0 to 1, 0.7 unless given) filled in the accent, a gap, then the rest in the accent's tint, with the handle at the level.
  function AndroidVolumeSlider(props) {
    var pct = (props.value != null && props.value !== '' ? Number(props.value) : 0.7) * 100 + '%';
    return h('div', { className: 'bae-a-volume', role: 'slider', 'aria-label': 'Volume', 'aria-valuenow': Math.round(parseFloat(pct)), style: { '--level': pct } }, h('div'), h('span'));
  }
  function AndroidCastRow(props) {
    // `waiting` with no receiver: the list's own wait, a spinner and its words (Looking for devices…).
    if (props.waiting && !props.name) return h('div', { className: 'bae-a-castrow' }, h(AndroidSpinner, { size: 20 }), h('span', { className: 'bae-muted', style: { fontSize: 'var(--type-android-body-size)' } }, props.waiting));
    // A receiver, in the picker and in the Playback settings alike: under its name its kind, or Playing for the one playing, which ends in Disconnect. Tapping any other plays on it. No separate "Playing on" row, and no check: the row says it. `menu` (the Playback settings) ends every row in ⋮ in place of Disconnect: Details… and, on the one playing, Disconnect; `menuOpen` draws that menu open under it.
    // `waiting` on a receiver: the words of a wait on it (Connecting…, or Accept on <the receiver>) in place of its kind, with a spinner at the row's end, before the ⋮ in the Playback settings.
    var icon = { cast: 'cast', airplay: 'speakerbox', upnp: 'tv' }[props.kind || 'cast'];
    return h('div', { className: 'bae-a-castrow' }, h(Glyph, { name: icon, size: 'large', className: props.active ? 'bae-accent' : 'bae-muted' }),
      h('span', { className: 'bae-a-castrow-text' }, h('span', { className: 'bae-a-castrow-name' }, props.name), props.waiting ? h('span', { className: 'bae-a-castrow-kind' }, props.waiting) : h('span', { className: props.active ? 'bae-a-castrow-state' : 'bae-a-castrow-kind' }, props.active ? 'Playing' : CAST_KIND[props.kind || 'cast'])),
      props.waiting ? h(AndroidSpinner, { size: 20 }) : null,
      props.menu ? h(AndroidMoreMenu, { open: props.menuOpen, items: props.active ? 'Details…,Disconnect' : 'Details…' }) : props.active ? h(AndroidButton, { kind: 'text' }, 'Disconnect') : null);
  }
  function AndroidCheckbox(props) {
    return h('div', { className: 'bae-a-checkrow', role: 'checkbox', 'aria-checked': !!props.on }, h('span', { className: cx('bae-a-checkbox', props.on && 'bae-a-checkbox-on') }, props.on ? h(Glyph, { name: 'check', size: 'small' }) : null), h('span', null, props.label));
  }


  // ---- Composer and work pages (macOS) ----
  function MacSectionHeader(props) {
    return h('div', { className: 'bae-m-sechead', style: { marginTop: Number(props.top || 0), marginBottom: Number(props.bottom || 0) } }, props.title);
  }
  // A work's detail: its title, then its sections (MacSectionHeader and MacLibraryRow) 12pt apart.
  function MacWorkDetail(props) {
    return h('div', { className: 'bae-m-work' }, h('div', { className: 'bae-m-work-title' }, props.title), props.children);
  }
  function MacHairline() { return h('div', { className: 'bae-m-hairline' }); }
  // The children are the actions, in a row under the text. No glyph sits above the title: the title says it.
  function MacContentUnavailable(props) {
    var actions = React.Children.toArray(props.children).filter(Boolean);
    return h('div', { className: 'bae-m-cu' },
      h('div', { className: 'bae-m-cu-title' }, props.title),
      props.description ? h('div', { className: 'bae-m-cu-desc' }, props.description) : null,
      actions.length ? h('div', { className: 'bae-m-cu-actions' }, actions) : null);
  }
