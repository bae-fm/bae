/* @ds-bundle: {"format":4,"namespace":"Bae","components":[{"name":"MacButton"},{"name":"MacTextField"},{"name":"MacRadio"},{"name":"MacPopUpButton"},{"name":"MacSwitch"},{"name":"MacAccentPicker"},{"name":"MacMenuItem"},{"name":"MacMenu"},{"name":"MacSegmentedControl"},{"name":"MacDisclosure"},{"name":"MacProgress"},{"name":"MacSpinner"},{"name":"MacErrorText"},{"name":"MacNote"},{"name":"MacRow"},{"name":"MacFieldRow"},{"name":"MacBox"},{"name":"MacSection"},{"name":"MacSheetFooter"},{"name":"MacSheet"},{"name":"MacAlert"},{"name":"MacSettingsWindow"},{"name":"MacWindow"},{"name":"AndroidButton"},{"name":"AndroidIconButton"},{"name":"AndroidSwitch"},{"name":"AndroidAccentPicker"},{"name":"AndroidTextField"},{"name":"AndroidRadio"},{"name":"AndroidNote"},{"name":"AndroidProgress"},{"name":"AndroidSpinner"},{"name":"AndroidRow"},{"name":"AndroidCard"},{"name":"AndroidSection"},{"name":"AndroidDialogActions"},{"name":"AndroidDialog"},{"name":"AndroidScreen"},{"name":"StatusChip"},{"name":"Notice"},{"name":"MacScanCode"},{"name":"AndroidScanCode"},{"name":"Artwork"},{"name":"MacMainWindow"},{"name":"MacNowPlayingBar"},{"name":"MacSeekBar"},{"name":"MacPlayPause"},{"name":"MacIconButton"},{"name":"MacLibraryHeading"},{"name":"MacAlbumGrid"},{"name":"MacAlbumTile"},{"name":"MacTrackRow"},{"name":"MacEyebrow"},{"name":"MacBrowseRow"},{"name":"MacDetailHeader"},{"name":"MacQueueRow"},{"name":"MacAlbumPanel"},{"name":"MacTrackColumns"},{"name":"MacBrowseSplit"},{"name":"MacArtistAlbumTile"},{"name":"MacSortCriteria"},{"name":"MacCheckbox"},{"name":"MacArtistGroupHeading"},{"name":"AndroidMenu"},{"name":"AndroidMenuItem"},{"name":"MacSearchCard"},{"name":"MacSearchSection"},{"name":"MacSearchRow"},{"name":"AndroidSearchSection"},{"name":"AndroidSearchRow"},{"name":"MacQueuePanel"},{"name":"MacQueueSection"},{"name":"MacQueuePlaceholderRow"},{"name":"MacQueueInsertLine"},{"name":"MacSidePause"},{"name":"MacCastPopover"},{"name":"AndroidSheet"},{"name":"AndroidQueueLabel"},{"name":"AndroidQueueRow"},{"name":"AndroidFullPlayer"},{"name":"AndroidCastRow"},{"name":"AndroidCheckbox"},{"name":"AndroidLibraryScreen"},{"name":"AndroidDetailScreen"},{"name":"AndroidNowPlayingBar"},{"name":"AndroidSeekBar"},{"name":"AndroidPlayPause"},{"name":"AndroidAlbumGrid"},{"name":"AndroidAlbumTile"},{"name":"AndroidArtistHeading"},{"name":"AndroidTrackRow"},{"name":"AndroidEyebrow"},{"name":"AndroidBrowseRow"},{"name":"AndroidAlbumHeader"},{"name":"AndroidDownloadControl"}]} */
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

  // A hairline between children.
  function divided(list) {
    var out = [];
    list.forEach(function (c, i) {
      if (i > 0 && !(c && c.props && c.props.nested)) out.push(h('div', { key: 'd' + i, className: 'bae-divider', 'aria-hidden': true }));
      out.push(c);
    });
    return out;
  }

  // A right-pointing chevron; a class turns it to point down.
  function Chevron(props) {
    return h('svg', { className: props.className, width: 7, height: 12, viewBox: '0 0 7 12', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 1.6, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M1.5 1.5 5.5 6l-4 4.5' }));
  }

  // Glyphs: the few the components draw. Stroke icons for macOS, Material paths for Android.
  function CopyGlyph() {
    return h('svg', { width: 14, height: 14, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('rect', { x: 9, y: 9, width: 11, height: 11, rx: 2 }),
      h('path', { d: 'M5 15V5a2 2 0 0 1 2-2h8' }));
  }
  function CheckGlyph() {
    return h('svg', { width: 14, height: 14, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
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
  function Material(props) {
    return h('svg', { width: props.size || 24, height: props.size || 24, viewBox: '0 0 24 24', 'aria-hidden': true, className: props.className },
      h('path', { d: MATERIAL[props.name] }));
  }

  // ---- macOS ----

  var PANES = ['Appearance', 'Library', 'Playback', 'Transfers', 'Import', 'Formats', 'Casting', 'Subsonic', 'Automation', 'About'];

  function MacWindow(props) {
    return h('div', { className: 'bae-m-window bae-m-window-plain', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      props.children);
  }

  // A macOS alert: centred title and message, its buttons stacked full width, the default one first.
  function MacAlert(props) {
    return h('div', { className: 'bae-m-scrim' },
      h('div', { role: 'alertdialog', 'aria-label': props.title, className: 'bae-m-alert' },
        h('div', { className: 'bae-m-alert-title' }, props.title),
        props.message ? h('div', { className: 'bae-m-alert-message' }, props.message) : null,
        h('div', { className: 'bae-m-alert-buttons' }, props.children)));
  }

  function MacSettingsWindow(props) {
    return h('div', { className: 'bae-m-window', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('nav', { className: 'bae-m-sidebar', 'aria-label': 'Settings panes' },
        h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
        PANES.map(function (p) {
          var flagged = props.status && p === props.statusPane;
          return h('button', { key: p, type: 'button', className: cx('bae-m-pane', p === props.pane && 'bae-m-pane-on') },
            h('span', null, p),
            flagged ? h(StatusChip, { tone: props.statusTone || 'warning' }, props.status) : null);
        })),
      h('main', { className: 'bae-m-main' },
        props.title ? h('div', { className: 'bae-m-heading' },
          props.back ? h('button', { type: 'button', className: 'bae-m-back', 'aria-label': 'Back to ' + props.back },
            h('svg', { width: 10, height: 10, viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: 'M15.41 16.59L10.83 12l4.58-4.59L14 6l-6 6 6 6 1.41-1.41z' }))) : null,
          h('h1', null, props.title)) : null,
        props.children));
  }

  function MacSection(props) {
    return h('section', { className: 'bae-m-section' },
      props.title ? h('div', { className: 'bae-m-section-title' }, props.title) : null,
      props.children);
  }

  function MacBox(props) {
    var list = kids(props.children);
    var out = divided(list);
    if (props.empty && list.length === 0) out.push(h('div', { key: 'e', className: 'bae-m-box-empty' }, props.empty));
    return h('div', { className: cx('bae-m-box', props.elevated && 'bae-m-box-elevated') }, out);
  }

  function MacRow(props) {
    var trail = kids(props.children);
    return h('div', { className: cx('bae-m-row', props.nested && 'bae-m-row-nested', props.selected && 'bae-m-row-selected', props.action && 'bae-m-row-action', props.destructive && 'bae-m-row-destructive'), onClick: props.onClick },
      props.label == null ? null : h('span', { className: 'bae-m-row-label' },
        h('span', { className: 'bae-m-row-title' }, h('span', { className: props.strong ? 'bae-m-row-strong' : null }, props.label), props.tag ? h(StatusChip, null, props.tag) : null),
        props.note ? h('span', { className: cx('bae-m-row-note', props.noteTone && 'bae-tone-' + props.noteTone) }, props.note) : null,
        props.waiting ? h('div', { className: 'bae-m-row-waiting bae-progress-track' }, h('div', { className: 'bae-progress-fill bae-progress-moving' })) : null),
      props.value != null && props.value !== '' ? h('span', { className: cx('bae-m-row-value', props.tone && 'bae-tone-' + props.tone) }, props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.value) : null,
      trail.length ? h('span', { className: 'bae-m-row-trail' }, trail) : null,
      props.chevron ? h(Chevron, { className: 'bae-m-row-chevron' }) : null);
  }

  // A row of a MacBox whose label takes a column labelWidth wide and whose field fills the rest.
  function MacFieldRow(props) {
    return h('div', { className: cx('bae-m-row', 'bae-m-fieldrow', props.alignText && 'bae-m-fieldrow-aligned') },
      props.label == null ? null : h('span', { className: 'bae-m-fieldrow-label', style: { width: Number(props.labelWidth || 90) } }, props.label),
      h('div', { className: 'bae-m-fieldrow-content' }, kids(props.children)));
  }

  function MacButton(props) {
    var kind = props.kind || 'secondary';
    var rest = {
      type: 'button',
      disabled: !!props.off,
      onClick: props.onClick,
      className: cx('bae-m-btn', 'bae-m-btn-' + kind, props.size && props.size !== 'regular' && 'bae-m-btn-' + props.size, props.destructive && 'bae-m-btn-destructive')
    };
    if (kind === 'icon') {
      var icon = props.icon || 'copy';
      rest['aria-label'] = props.label || icon;
      return h('button', rest, icon === 'check' ? h(CheckGlyph) : h(CopyGlyph));
    }
    return h('button', rest, props.children != null ? props.children : props.label);
  }

  function MacTextField(props) {
    var state = props.state || 'normal';
    var hidden = !!props.labelHidden;
    var bare = hidden || props.label == null;
    var lw = props.labelWidth != null ? Number(props.labelWidth) : 90;
    var isError = state === 'error';
    var line = isError ? props.error : props.note;
    var input = h('input', {
      className: cx('bae-m-input', state === 'focused' && 'bae-m-input-focused', isError && 'bae-m-input-error'),
      type: props.secret ? 'password' : 'text',
      defaultValue: props.value || '',
      placeholder: hidden ? props.label : props.placeholder,
      'aria-label': props.label,
      disabled: state === 'off',
      spellCheck: false
    });
    return h('div', { className: cx('bae-m-field', state === 'off' && 'bae-off') },
      h('div', { className: 'bae-m-field-line' },
        bare ? null : h('span', { className: 'bae-m-field-label', style: { width: lw } }, props.label),
        input,
        props.children),
      line ? h('div', { className: cx('bae-m-field-note', isError && 'bae-m-field-note-error'), style: { paddingLeft: bare ? 0 : lw + 12 } }, line) : null);
  }

  function MacRadio(props) {
    return h('div', { className: 'bae-m-row bae-m-radio', role: 'radio', 'aria-checked': !!props.checked, onClick: props.onClick },
      h('span', { className: cx('bae-m-radio-dot', props.checked && 'bae-m-radio-dot-on'), 'aria-hidden': true }),
      h('span', { className: 'bae-m-row-label' },
        props.label,
        props.note ? h('span', { className: 'bae-m-row-note' }, props.note) : null));
  }

  function MacPopUpButton(props) {
    return h('button', { type: 'button', className: 'bae-m-popupbutton', disabled: !!props.off, style: props.width ? { width: Number(props.width) } : null },
      h('span', null, props.children != null ? props.children : props.value),
      h('svg', { className: 'bae-m-popupbutton-mark', width: 8, height: 12, viewBox: '0 0 8 12', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M1.5 4.5 4 2l2.5 2.5M1.5 7.5 4 10l2.5-2.5' })));
  }

  function MacSegmentedControl(props) {
    var options = String(props.options || '').split(',').map(function (o) { return o.trim(); }).filter(Boolean);
    return h('span', { className: 'bae-m-segmented', role: 'radiogroup' },
      options.map(function (o) {
        return h('span', { key: o, role: 'radio', 'aria-checked': o === props.selected, className: cx('bae-m-segment', o === props.selected && 'bae-m-segment-on') }, o);
      }));
  }

  function MacDisclosure(props) {
    var s = React.useState(!!props.open);
    var open = s[0];
    return h('div', { className: 'bae-m-disclosure' },
      h('button', { type: 'button', className: 'bae-m-disclosure-toggle', 'aria-expanded': open, onClick: function () { s[1](!open); } },
        h(Chevron, { className: cx('bae-m-disclosure-mark', open && 'bae-m-disclosure-mark-open') }),
        props.label || 'Advanced'),
      open ? props.children : null);
  }

  function MacSheet(props) {
    var panel = h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-m-sheet', style: { width: Number(props.width || 460) } },
      props.title ? h('div', { className: 'bae-m-sheet-title' }, props.title) : null,
      props.title ? h('div', { className: 'bae-divider', 'aria-hidden': true }) : null,
      h('div', { className: 'bae-m-sheet-body' }, props.children));
    if (!props.overlay) return panel;
    return h('div', { className: 'bae-m-scrim' }, panel);
  }

  // split sets the first child apart on the left, in its own box so a wrapper around it can't swallow the spacing.
  function MacSheetFooter(props) {
    var list = kids(props.children);
    if (!props.split || list.length < 2) return h('div', { className: 'bae-m-sheet-footer' }, list);
    return h('div', { className: 'bae-m-sheet-footer' },
      h('div', { className: 'bae-m-sheet-footer-lead' }, list[0]), list.slice(1));
  }

  function Progress(prefix) {
    return function (props) {
      var known = typeof props.value === 'number';
      return h('div', { className: prefix + '-progress' },
        props.label ? h('div', null, props.label) : null,
        h('div', { className: 'bae-progress-track', role: 'progressbar', 'aria-valuenow': known ? Math.round(props.value * 100) : undefined },
          h('div', { className: cx('bae-progress-fill', !known && 'bae-progress-moving'), style: known ? { width: (props.value * 100) + '%' } : null })));
    };
  }
  function Spinner(prefix, size) {
    return function (props) {
      var s = Number(props.size || size);
      var spin = h('svg', { className: 'bae-spinner', width: s, height: s, viewBox: '0 0 24 24', role: 'progressbar', 'aria-label': props.label || 'Loading', fill: 'none' },
        h('circle', { cx: 12, cy: 12, r: 9, stroke: 'currentColor', strokeOpacity: 0.2, strokeWidth: 2.5 }),
        h('path', { d: 'M12 3a9 9 0 0 1 9 9', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' }));
      return props.label ? h('span', { className: prefix + '-spinner' }, spin, h('span', null, props.label)) : h('span', { className: prefix + '-spinner' }, spin);
    };
  }
  var MacSpinner = Spinner('bae-m', 16);
  var AndroidSpinner = Spinner('bae-a', 24);
  var MacProgress = Progress('bae-m');
  var AndroidProgress = Progress('bae-a');

  function MacErrorText(props) {
    return h('div', { className: 'bae-m-error' }, props.children);
  }

  function MacNote(props) {
    return h('div', { className: 'bae-m-note' }, props.children);
  }

  function MacSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-m-switch', on && 'bae-m-switch-on'), onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-m-switch-knob' }));
  }

  // The accents in the order the appearance settings offer them.
  var ACCENTS = ['blue', 'indigo', 'purple', 'pink', 'red', 'amber', 'green', 'teal'];

  function accentPicker(className) {
    return function (props) {
      var s = React.useState(props.selected || 'pink');
      var chosen = s[0];
      return h('div', { className: className, role: 'radiogroup', 'aria-label': props.label || 'Accent' }, ACCENTS.map(function (a) {
        return h('button', { key: a, type: 'button', role: 'radio', 'aria-checked': a === chosen, 'aria-label': a.charAt(0).toUpperCase() + a.slice(1),
          className: cx('bae-swatch', a === chosen && 'bae-swatch-on'), style: { background: 'var(--accent-' + a + '-fill)' }, onClick: function () { s[1](a); } });
      }));
    };
  }
  var MacAccentPicker = accentPicker('bae-m-accents');
  var AndroidAccentPicker = accentPicker('bae-a-accents');

  function MacMenu(props) {
    return h('div', { className: 'bae-m-menu', role: 'menu' }, props.children);
  }

  function MacMenuItem(props) {
    return h('div', { className: cx('bae-m-menu-item', props.highlighted && 'bae-m-menu-item-on'), role: 'menuitemradio', 'aria-checked': !!props.checked },
      h('span', { className: 'bae-m-menu-check', 'aria-hidden': true }, props.checked ? h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.6, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M5 12.5l4.5 4.5L19 7.5' })) : null),
      props.children);
  }

  // ---- Android ----

  function AndroidScreen(props) {
    return h('div', { className: 'bae-a-screen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      props.title ? h('header', { className: 'bae-a-topbar' },
        props.back ? h(AndroidIconButton, { icon: 'back', label: 'Back' }) : null,
        h('h1', null, props.title)) : null,
      h('main', { className: 'bae-a-main' }, props.children));
  }

  function AndroidSection(props) {
    return h('h2', { className: 'bae-a-section' }, props.title || props.children);
  }

  function AndroidCard(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-a-card' }, divided(list));
  }

  function AndroidRow(props) {
    var trail = kids(props.children);
    var lines = h('div', { className: 'bae-a-row-lines' },
      h('div', { className: cx('bae-a-row-title', props.action && 'bae-a-row-action', props.destructive && 'bae-a-row-destructive') }, props.title, props.tag ? h(StatusChip, null, props.tag) : null),
      props.subtitle ? h('div', { className: cx('bae-a-row-subtitle', props.tone && 'bae-tone-' + props.tone) },
        props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.subtitle) : null);
    return h('div', { className: cx('bae-a-row', props.nested && 'bae-a-row-nested', !props.subtitle && 'bae-a-row-one', props.selected && 'bae-a-row-selected'), role: props.onClick ? 'button' : null, onClick: props.onClick },
      props.leadingIcon ? h(Material, { name: props.leadingIcon, className: cx('bae-a-row-lead', props.action && 'bae-a-row-lead-action') }) : null,
      lines,
      trail.length ? h('span', { className: 'bae-a-row-trail' }, trail) : null,
      props.icon ? h(Material, { name: props.icon, size: props.icon === 'edit' ? 20 : 24, className: 'bae-a-row-icon' }) : null);
  }

  function AndroidButton(props) {
    var kind = props.kind || 'filled';
    return h('button', { type: 'button', disabled: !!props.off, onClick: props.onClick,
      className: cx('bae-a-btn', 'bae-a-btn-' + kind, props.large && 'bae-a-btn-large', props.destructive && 'bae-a-btn-destructive', props.alignTo && 'bae-a-btn-align-' + props.alignTo) },
      props.children != null ? props.children : props.label);
  }

  function AndroidIconButton(props) {
    return h('button', { type: 'button', className: 'bae-a-iconbtn', 'aria-label': props.label || props.icon },
      h(Material, { name: props.icon || 'more' }));
  }

  function AndroidSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-a-switch', on && 'bae-a-switch-on'), onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-a-switch-thumb' }));
  }

  function AndroidTextField(props) {
    var state = props.state || 'normal';
    var isError = state === 'error';
    var line = isError ? props.error : props.helper;
    return h('div', { className: cx('bae-a-field', state === 'focused' && 'bae-a-field-focused', isError && 'bae-a-field-error') },
      h('label', { className: 'bae-a-field-box' },
        h('span', { className: 'bae-a-field-label' }, props.label),
        h('input', { className: 'bae-a-input', defaultValue: props.value || '', 'aria-label': props.label, spellCheck: false })),
      line ? h('div', { className: 'bae-a-field-line' }, line) : null);
  }

  function AndroidRadio(props) {
    return h('label', { className: 'bae-a-radio', role: 'radio', 'aria-checked': !!props.checked },
      h('span', { className: cx('bae-a-radio-dot', props.checked && 'bae-a-radio-dot-on'), 'aria-hidden': true }),
      props.label);
  }

  function AndroidDialog(props) {
    return h('div', { className: 'bae-a-scrim' },
      h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-a-dialog', style: { width: Number(props.width || 312) } },
        props.title ? h('h2', { className: 'bae-a-dialog-title' }, props.title) : null,
        props.children));
  }

  function AndroidDialogActions(props) {
    return h('div', { className: 'bae-a-dialog-actions' }, props.children);
  }

  function AndroidNote(props) {
    return h('div', { className: 'bae-a-note' }, props.children);
  }

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


  // ---- Library and playback ----

  // A stand-in cover: each title gets its own two-color square, so boards read like a library.
  // `missing` is a cover the release doesn't have: a plain tile, no glyph.
  function hueOf(s) { var x = 0; s = String(s || ''); for (var i = 0; i < s.length; i++) x = (x * 31 + s.charCodeAt(i)) % 360; return x; }
  function Artwork(props) {
    var size = props.size != null ? Number(props.size) : null;
    var hue = props.hue != null ? Number(props.hue) : hueOf(props.seed || props.title);
    var style = { borderRadius: Number(props.radius != null ? props.radius : 10) };
    if (size) { style.width = size; style.height = size; }
    if (props.round) style.borderRadius = '50%';
    if (!props.missing) style.backgroundImage = 'linear-gradient(135deg, hsl(' + hue + ' 42% 52%), hsl(' + ((hue + 48) % 360) + ' 46% 24%))';
    return h('div', { className: cx('bae-art', props.missing && 'bae-art-missing', props.shadow && 'bae-art-shadow-' + props.shadow, props.className), style: style, role: 'img', 'aria-label': props.label || props.title || 'Cover' }, props.children);
  }

  var SF = {
    play: 'M8 5v14l11-7z', pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
    previous: 'M11 18V6l-8.5 6 8.5 6zm.5-6l8.5 6V6l-8.5 6z', next: 'M4 18l8.5-6L4 6v12zm9-12v12l8.5-6L13 6z',
    shuffle: 'M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z',
    repeat: 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
    queue: 'M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z',
    speaker: 'M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0014 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z',
    cast: 'M7 2h10a2 2 0 012 2v16a2 2 0 01-2 2H7a2 2 0 01-2-2V4a2 2 0 012-2zm5 2.5a1.75 1.75 0 100 3.5 1.75 1.75 0 000-3.5zm0 5.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9zm0 2.25a2.25 2.25 0 110 4.5 2.25 2.25 0 010-4.5z',
    gear: 'M19.14 12.94a7.07 7.07 0 000-1.88l2.03-1.58a.5.5 0 00.12-.61l-1.92-3.32a.5.5 0 00-.6-.22l-2.39.96a7 7 0 00-1.62-.94l-.36-2.54A.5.5 0 0013.9 2.5h-3.84a.5.5 0 00-.49.42l-.36 2.54c-.59.24-1.13.56-1.62.94l-2.39-.96a.5.5 0 00-.6.22L2.68 8.98a.5.5 0 00.12.61l2.03 1.58a7.07 7.07 0 000 1.88l-2.03 1.58a.5.5 0 00-.12.61l1.92 3.32c.13.22.39.3.6.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.25.42.49.42h3.84c.24 0 .45-.18.49-.42l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .6-.22l1.92-3.32a.5.5 0 00-.12-.61l-2.03-1.58zM12 15.6a3.6 3.6 0 110-7.2 3.6 3.6 0 010 7.2z',
    search: 'M15.5 14h-.79l-.28-.27A6.47 6.47 0 0016 9.5 6.5 6.5 0 109.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
    ellipsis: 'M6 10a2 2 0 100 4 2 2 0 000-4zm6 0a2 2 0 100 4 2 2 0 000-4zm6 0a2 2 0 100 4 2 2 0 000-4z',
    warning: 'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
    close: 'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
    check: 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
    airplay: 'M6 22h12l-6-6-6 6zM21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h4v-2H3V5h18v12h-4v2h4c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z',
    tv: 'M21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h5v2h8v-2h5c1.1 0 1.99-.9 1.99-2L23 5c0-1.1-.9-2-2-2zm0 14H3V5h18v12z'
  };
  var MAT = {
    play: 'M8 5v14l11-7z', pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
    previous: 'M6 6h2v12H6zm3.5 6l8.5 6V6z', next: 'M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z',
    cast: 'M21 3H3c-1.1 0-2 .9-2 2v3h2V5h18v14h-7v2h7c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM1 18v3h3c0-1.66-1.34-3-3-3zm0-4v2c2.76 0 5 2.24 5 5h2c0-3.87-3.13-7-7-7zm0-4v2c4.97 0 9 4.03 9 9h2c0-6.08-4.93-11-11-11z',
    queue: 'M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z',
    repeat: 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
    nowplaying: 'M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0014 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z',
    down: 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z',
    search: MATERIAL.search, more: MATERIAL.more, back: MATERIAL.back,
    warning: 'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
    download: 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z',
    downloaded: 'M5 18h14v2H5v-2zm4.6-2.7L5 10.7l2-1.9 2.6 2.6L17 4l2 2-9.4 9.3z',
    close: 'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
    retry: MATERIAL.refresh,
    remove: 'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z',
    drag: 'M20 9H4v2h16V9zM4 15h16v-2H4v2z',
    shuffle: 'M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z',
    check: 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
    speakerbox: 'M17 2H7c-1.1 0-2 .9-2 2v16c0 1.1.9 1.99 2 1.99L17 22c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm-5 2c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zm0 16c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z',
    tv: 'M21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h5v2h8v-2h5c1.1 0 1.99-.9 1.99-2L23 5c0-1.1-.9-2-2-2zm0 14H3V5h18v12z'
  };
  function Glyph(props) {
    var set = props.set === 'mac' ? SF : MAT;
    var s = Number(props.size || 24);
    return h('svg', { width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true, className: cx('bae-glyph', props.className), fill: 'currentColor' }, h('path', { d: set[props.name] || set.ellipsis || '', fillRule: 'evenodd' }));
  }

  // Position and the parts this device holds (`held`, "0-0.4,0.6-0.7"), drawn faint under what has played.
  function heldRanges(held) {
    if (held == null || held === '') return [];
    if (typeof held === 'number') return [[0, held]];
    return String(held).split(',').map(function (r) { var p = r.split('-'); return [Number(p[0]), Number(p[1])]; });
  }
  function SeekLine(prefix) {
    return function (props) {
      var pos = Number(props.position || 0);
      return h('div', { className: cx('bae-seek', prefix + '-seek', props.off && 'bae-seek-off') },
        h('span', { className: 'bae-seek-time' }, props.elapsed || '0:00'),
        h('div', { className: 'bae-seek-track', role: 'slider', 'aria-label': 'Position', 'aria-valuenow': Math.round(pos * 100) },
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
    return h('button', { type: 'button', className: cx('bae-m-iconbtn', props.active && 'bae-m-iconbtn-on'), 'aria-label': props.label || props.icon, title: props.label },
      h(Glyph, { set: 'mac', name: props.icon, size: props.size || 17 }));
  }

  function MacPlayPause(props) {
    return h('span', { className: 'bae-m-playpause' },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: 48, height: 48, viewBox: '0 0 48 48', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 20 }));
  }

  function MacNowPlayingBar(props) {
    var none = !props.title;
    return h('div', { className: 'bae-m-npbar' },
      h('div', { className: 'bae-m-np-left' }, none ? null : [
        h(Artwork, { key: 'a', title: props.album || props.title, size: 56, radius: 6, shadow: 'bar', missing: props.noCover }),
        h('div', { key: 't', className: 'bae-m-np-text' }, h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist))]),
      h('div', { className: 'bae-m-np-center' },
        h('div', { className: 'bae-m-np-transport' },
          h(MacIconButton, { icon: 'shuffle', label: 'Shuffle', active: props.shuffle }),
          h(MacIconButton, { icon: 'previous', label: 'Previous' }),
          h(MacPlayPause, { playing: props.playing, waiting: props.waiting }),
          h(MacIconButton, { icon: 'next', label: 'Next' }),
          h(MacIconButton, { icon: 'repeat', label: 'Repeat', active: props.repeat })),
        h(MacSeekBar, { position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, off: none })),
      h('div', { className: 'bae-m-np-right' },
        h(MacIconButton, { icon: 'cast', label: props.casting ? 'Casting to ' + props.casting : 'Cast', active: !!props.casting }),
        h(MacIconButton, { icon: 'queue', label: 'Queue', active: props.queueOpen }),
        h(MacIconButton, { icon: 'speaker', label: 'Mute' }),
        h('div', { className: 'bae-m-volume' }, h('div', { style: { width: ((props.volume != null ? Number(props.volume) : 0.7) * 100) + '%' } }))));
  }

  function MacMainWindow(props) {
    var section = props.section || 'Library';
    return h('div', { className: 'bae-m-window bae-m-main-window', style: { width: Number(props.width || 1200), height: Number(props.height || 760) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      h('header', { className: 'bae-m-titlebar' },
        h('div', { className: 'bae-m-sections', role: 'tablist' }, ['Library', 'Import'].map(function (s) {
          return h('span', { key: s, role: 'tab', 'aria-selected': s === section, className: cx('bae-m-section-tab', s === section && 'bae-m-section-tab-on') }, s);
        })),
        h('div', { className: 'bae-m-titlebar-trail' },
          h('div', { className: cx('bae-m-search', props.searchFocused && 'bae-m-search-focused') }, h(Glyph, { set: 'mac', name: 'search', size: 14 }), h('span', { className: props.search ? null : 'bae-m-search-prompt' }, props.search || 'Search')),
          props.sync === 'moving' ? h('span', { className: 'bae-m-iconbtn', title: 'Sync activity' }, h(MacSpinner, { size: 14 })) : null,
          props.sync === 'attention' ? h('span', { className: 'bae-m-iconbtn bae-m-sync-warn', title: 'Sync activity' }, h(Glyph, { set: 'mac', name: 'warning', size: 13 })) : null,
          h(MacIconButton, { icon: 'gear', label: 'Settings' }))),
      h('div', { className: 'bae-m-main-body' }, h('div', { className: 'bae-m-main-content' }, kids(props.children).filter(function (c) { return !(c && c.type === MacQueuePanel); })), kids(props.children).filter(function (c) { return c && c.type === MacQueuePanel; })),
      h(MacNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, shuffle: props.shuffle, repeat: props.repeat, casting: props.casting, queueOpen: props.queueOpen }));
  }

  function MacLibraryHeading(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: cx('bae-m-libhead', collapsed && 'bae-m-libhead-collapsed') },
      h('h1', { className: 'bae-m-libhead-title' }, props.view || 'Albums', h('span', { className: 'bae-m-libhead-chevron', 'aria-hidden': true }, h(Glyph, { set: 'mac', name: 'down', size: collapsed ? 8 : 15 }))),
      h('div', { className: 'bae-m-libhead-trail' }, props.children));
  }
  SF.down = 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z';

  function MacAlbumGrid(props) {
    var n = Number(props.columns || 4);
    return h('div', { className: 'bae-m-grid', style: { gridTemplateColumns: 'repeat(' + n + ', minmax(0, 1fr))' } }, props.children);
  }

  function MacAlbumTile(props) {
    return h('div', { className: cx('bae-m-tile', props.selected && 'bae-m-tile-selected', props.open && 'bae-m-tile-open') },
      h('div', { className: 'bae-m-tile-art' },
        h(Artwork, { title: props.title, missing: props.noCover, radius: 10, shadow: 'tile' }),
        props.hover ? h('span', { className: 'bae-m-tile-menu', 'aria-label': 'Album options' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 13 })) : null),
      h('div', { className: 'bae-m-tile-title' }, props.title),
      h('div', { className: 'bae-m-tile-artist' }, props.artist),
      h('div', { className: 'bae-m-tile-year' }, props.year || ' '));
  }

  function MacTrackRow(props) {
    var lead = props.hover ? h(Glyph, { set: 'mac', name: props.current && props.playing ? 'pause' : 'play', size: 13 })
      : props.current ? h(Glyph, { set: 'mac', name: 'speaker', size: 11, className: 'bae-accent' })
      : h('span', { className: 'bae-m-track-pos' }, props.position);
    return h('div', { className: cx('bae-m-track', props.hover && 'bae-m-track-hover', props.artist && 'bae-m-track-two') },
      h('span', { className: 'bae-m-track-lead' }, lead),
      h('div', { className: 'bae-m-track-text' },
        h('div', { className: cx('bae-m-track-title', props.current && 'bae-accent') }, props.title),
        props.artist ? h('div', { className: 'bae-m-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-m-track-time' }, props.duration));
  }

  function MacEyebrow(props) { return h('div', { className: 'bae-m-eyebrow' }, props.children); }

  function MacBrowseRow(props) {
    if (props.loading) return h('div', { className: 'bae-m-browse bae-m-browse-loading' }, h('div', { className: 'bae-m-browse-art bae-ph' }), h('div', { className: 'bae-m-browse-text' }, h('div', { className: 'bae-ph', style: { width: 140, height: 11 } }), h('div', { className: 'bae-ph', style: { width: 80, height: 10 } })));
    return h('div', { className: cx('bae-m-browse', props.selected && 'bae-m-browse-selected', props.hover && 'bae-m-browse-hover') },
      h(Artwork, { title: props.name, size: 44, radius: 6, missing: props.noImage }),
      h('div', { className: 'bae-m-browse-text' }, h('div', { className: 'bae-m-browse-name' }, props.name), h('div', { className: 'bae-m-browse-count' }, props.count)));
  }

  function MacDetailHeader(props) {
    return h('div', { className: 'bae-m-dethead' },
      h(Artwork, { title: props.name, size: 72, radius: 10, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-m-dethead-name' }, props.name), h('div', { className: 'bae-m-dethead-count' }, props.count)));
  }

  function MacQueueRow(props) {
    return h('div', { className: cx('bae-m-qrow', props.hover && 'bae-m-qrow-hover') },
      h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }, props.hover ? h('span', { className: 'bae-m-qrow-play' }, h(Glyph, { set: 'mac', name: 'play', size: 11 })) : null),
      h('div', { className: 'bae-m-qrow-text' }, h('div', { className: 'bae-m-qrow-title' }, props.title), h('div', { className: 'bae-m-qrow-sub' }, props.artist), h('div', { className: 'bae-m-qrow-sub' }, props.album)),
      props.hover ? h('span', { className: 'bae-m-qrow-remove', title: 'Remove from queue' }, h(Glyph, { set: 'mac', name: 'close', size: 11 })) : h('span', { className: 'bae-m-qrow-time' }, props.duration));
  }


  // The album opened in the grid: a panel under the album's row.
  function MacAlbumPanel(props) {
    var releases = props.releases ? String(props.releases).split(',').map(function (r) { return r.trim(); }) : null;
    return h('section', { className: 'bae-m-albumpanel' },
      h('span', { className: 'bae-m-albumpanel-close', 'aria-label': 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 13 })),
      h(Artwork, { title: props.title, size: Number(props.coverSize || 340), radius: 10, missing: props.noCover, className: 'bae-m-albumpanel-cover' }),
      h('div', { className: 'bae-m-albumpanel-body' },
        h('h2', { className: 'bae-m-albumpanel-title' }, props.title),
        h('div', { className: 'bae-m-albumpanel-sub' }, h('span', { className: 'bae-muted' }, props.artist), props.year ? [h('span', { key: 'd', className: 'bae-m-tertiary' }, '·'), h('span', { key: 'y', className: 'bae-m-tertiary' }, props.year)] : null),
        releases && releases.length > 1 ? h('div', { className: 'bae-m-albumpanel-releases' }, h(MacSegmentedControl, { options: releases.join(','), selected: props.release || releases[0] })) : null,
        props.pressing || props.labels ? h('div', { className: 'bae-m-albumpanel-facts' }, props.pressing ? h('div', null, props.pressing) : null, props.labels ? h('div', null, props.labels) : null) : null,
        props.unplayable ? h('div', { className: 'bae-m-albumpanel-unplayable' }, props.unplayable) : null,
        h('div', { className: 'bae-m-albumpanel-actions' }, h(MacButton, { kind: 'primary' }, 'Play'), h('span', { className: 'bae-m-albumpanel-more', 'aria-label': 'Album options' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 13 }))),
        h('div', { className: 'bae-m-albumpanel-tracks' }, props.children)));
  }

  // A side's tracks in two columns once it has more than eight.
  function MacTrackColumns(props) {
    return h('div', { className: 'bae-m-trackcols' }, props.children);
  }

  // Artists and composers: the list on the left, the selected one's page on the right.
  function MacBrowseSplit(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-m-split' },
      h('div', { className: 'bae-m-split-list' }, list[0]),
      h('div', { className: 'bae-m-split-detail' }, list.slice(1)));
  }

  function MacArtistAlbumTile(props) {
    return h('div', { className: 'bae-m-atile' }, h(Artwork, { title: props.title, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-m-tile-title' }, props.title), h('div', { className: 'bae-m-atile-year' }, props.year || ' '));
  }


  // A sort criterion: field menu, direction, remove (when there are several); then + to add one.
  function MacSortCriteria(props) {
    var items = String(props.criteria || 'Date Added:down').split(',').map(function (c) { var p = c.trim().split(':'); return { field: p[0], dir: p[1] || 'up' }; });
    var arrow = function (d) { return h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2.4, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: d === 'down' ? 'M12 4v16M5 13l7 7 7-7' : 'M12 20V4M5 11l7-7 7 7' })); };
    return h('div', { className: 'bae-m-sortrow' },
      items.map(function (it, i) {
        return h('span', { key: i, className: 'bae-m-sortpill' },
          h('span', { className: 'bae-m-sortpill-field' }, it.field, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-muted' })),
          h('span', { className: 'bae-muted', 'aria-label': it.dir === 'down' ? 'Descending' : 'Ascending' }, arrow(it.dir)),
          items.length > 1 ? h('span', { className: 'bae-muted', 'aria-label': 'Remove sort criterion' }, h(Glyph, { set: 'mac', name: 'close', size: 8 })) : null);
      }),
      props.full ? null : h('span', { className: 'bae-m-sortadd', 'aria-label': 'Add sort criterion' }, h(Glyph, { set: 'mac', name: 'add', size: 13 })));
  }
  SF.add = 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z';

  function MacCheckbox(props) {
    return h('label', { className: 'bae-m-checkbox' }, h('span', { className: cx('bae-m-checkbox-box', props.on && 'bae-m-checkbox-on') }, props.on ? h(CheckGlyph) : null), props.label);
  }

  // Names an artist above their albums when the grid is grouped by artist.
  function MacArtistGroupHeading(props) {
    return h('div', { className: 'bae-m-groupheading' }, props.name);
  }

  // A Material menu: `AndroidMenuItem`s, a "Sort by" style label, dividers.
  function AndroidMenu(props) {
    var style = { position: 'absolute', top: Number(props.top || 56), minWidth: Number(props.width || 200) };
    if (props.left != null) style.left = Number(props.left); else style.right = Number(props.right != null ? props.right : 8);
    return h('div', { className: 'bae-a-menu', role: 'menu', style: style }, props.children);
  }
  function AndroidMenuItem(props) {
    if (props.divider) return h('div', { className: 'bae-a-menu-divider', role: 'separator' });
    if (props.heading) return h('div', { className: 'bae-a-menu-heading' }, props.children);
    return h('div', { className: cx('bae-a-menu-item', props.selected && 'bae-a-menu-item-on', props.highlighted && 'bae-a-menu-item-hl'), role: 'menuitem' },
      h('span', null, props.children),
      props.check != null ? h('span', { className: 'bae-a-menu-check', style: { opacity: props.check ? 1 : 0 } }, h(Material, { name: 'check', size: 24 })) : null);
  }
  MATERIAL.check = 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z';

  // ---- Android library ----

  function AndroidIcon(props) {
    return h('button', { type: 'button', className: cx('bae-a-iconbtn', props.className), 'aria-label': props.label || props.icon }, h(Glyph, { name: props.icon, size: props.size || 24 }));
  }

  function AndroidPlayPause(props) {
    var icon = Number(props.size || 24), box = icon + 16;
    return h('span', { className: 'bae-a-playpause', style: { width: box, height: box } },
      props.waiting ? h('svg', { className: 'bae-waitarc', width: box, height: box, viewBox: '0 0 48 48', 'aria-label': 'Downloading the track…' }, h('path', { d: 'M24 1.25A22.75 22.75 0 0 1 46.75 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' })) : null,
      h(Glyph, { name: props.playing || props.waiting ? 'pause' : 'play', size: icon }));
  }

  function AndroidNowPlayingBar(props) {
    if (!props.title) return null;
    return h('div', { className: 'bae-a-npbar' },
      h('div', { className: 'bae-a-np-row' },
        h('div', { className: 'bae-a-np-info' }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
          h('div', { className: 'bae-a-np-text' }, h('div', { className: 'bae-a-np-title' }, props.title), h('div', { className: 'bae-a-np-sub' }, props.artist))),
        h(AndroidIcon, { icon: 'previous', label: 'Previous' }),
        h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting }),
        h(AndroidIcon, { icon: 'next', label: 'Next' }),
        props.castOff ? null : h(AndroidIcon, { icon: 'cast', label: 'Cast', className: props.casting ? 'bae-a-icon-accent' : 'bae-a-icon-muted' }),
        h(AndroidIcon, { icon: 'queue', label: 'Queue' }),
        h(AndroidIcon, { icon: 'repeat', label: 'Repeat', className: props.repeat ? 'bae-a-icon-accent' : 'bae-a-icon-muted' })),
      h(AndroidSeekBar, { position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration }));
  }

  function AndroidLibraryScreen(props) {
    var collapsed = !!props.collapsed;
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      props.searching ? h('header', { className: 'bae-a-searchbar' }, h(AndroidIcon, { icon: 'back', label: 'Back' }),
          h('div', { className: 'bae-a-searchfield' }, h('span', { className: props.search ? null : 'bae-a-search-prompt' }, props.search || 'Search albums, tracks, composers, and works'), props.search ? h(Glyph, { name: 'close', size: 20 }) : null))
        : h('header', { className: cx('bae-a-libheader', collapsed && 'bae-a-libheader-collapsed') },
          h('h1', { className: 'bae-a-libhead-title' }, h('span', null, props.view || 'Albums'), h(Glyph, { name: 'down', size: collapsed ? 16 : 20, className: 'bae-a-libhead-chevron' })),
          h(AndroidIcon, { icon: 'search', label: 'Search' }),
          props.sync === 'moving' ? h('span', { className: 'bae-a-iconbtn' }, h(AndroidSpinner, { size: 16 })) : null,
          props.sync === 'attention' ? h(AndroidIcon, { icon: 'warning', label: 'Sync activity', className: 'bae-a-sync-warn' }) : null,
          h(AndroidIcon, { icon: 'more', label: 'More options' })),
      h('main', { className: 'bae-a-libbody' }, props.children),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }));
  }

  function AndroidDetailScreen(props) {
    return h('div', { className: 'bae-a-screen bae-a-libscreen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      h('header', { className: 'bae-a-detailbar' },
        h('button', { type: 'button', className: 'bae-a-detail-back' }, h(Glyph, { name: 'back', size: 24 }), h('span', null, props.back || 'Library')),
        h('span', { style: { flex: 1 } }),
        props.menu ? h(AndroidIcon, { icon: 'more', label: 'Album options' }) : null),
      h('main', { className: props.flush ? 'bae-a-libbody' : 'bae-a-libbody bae-a-detailbody' }, props.children),
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat }));
  }

  function AndroidAlbumGrid(props) {
    return h('div', { className: 'bae-a-grid' }, props.children);
  }

  function AndroidAlbumTile(props) {
    return h('div', { className: 'bae-a-tile' },
      h(Artwork, { title: props.title, missing: props.noCover, radius: 10 }),
      h('div', { className: 'bae-a-tile-title' }, props.title),
      h('div', { className: 'bae-a-tile-line' }, props.line));
  }

  function AndroidArtistHeading(props) {
    if (props.pinned) return h('div', { className: 'bae-a-artisthead-pinned' }, h(Artwork, { title: props.name, size: 32, round: true, missing: props.noImage }), h('div', { className: 'bae-a-artisthead-name' }, props.name));
    return h('div', { className: 'bae-a-artisthead' }, h(Artwork, { title: props.name, size: 48, round: true, missing: props.noImage }),
      h('div', null, h('div', { className: 'bae-a-artisthead-name' }, props.name), h('div', { className: 'bae-a-artisthead-count' }, props.count)));
  }

  function AndroidTrackRow(props) {
    return h('div', { className: 'bae-a-track' },
      h('span', { className: 'bae-a-track-lead' }, props.current ? h(Glyph, { name: props.playing ? 'nowplaying' : 'play', size: 24, className: 'bae-accent' }) : h('span', { className: 'bae-a-track-pos' }, props.position)),
      h('div', { className: 'bae-a-track-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), props.artist ? h('div', { className: 'bae-a-track-artist' }, props.artist) : null),
      h('span', { className: 'bae-a-track-time' }, props.duration),
      h('span', { className: 'bae-a-iconbtn bae-a-icon-muted', 'aria-label': 'Track options' }, h(Glyph, { name: 'more', size: 24 })));
  }

  function AndroidEyebrow(props) { return h('div', { className: 'bae-a-eyebrow' }, props.children); }

  function AndroidBrowseRow(props) {
    return h('div', { className: 'bae-a-browse' }, h(Artwork, { title: props.name, size: 48, radius: 6, missing: props.noImage }),
      h('div', { className: 'bae-a-browse-text' }, h('div', { className: 'bae-a-browse-name' }, props.name), h('div', { className: 'bae-a-browse-count' }, props.count)));
  }

  function AndroidAlbumHeader(props) {
    return h('div', { className: 'bae-a-albumhead' }, h(Artwork, { title: props.title, size: 140, radius: 10, missing: props.noCover }),
      h('div', { className: 'bae-a-albumhead-text' }, h('div', { className: 'bae-a-albumhead-title' }, props.title), h('div', { className: 'bae-a-albumhead-artist' }, props.artist), h('div', { className: 'bae-a-albumhead-fact' }, props.year),
        props.pressing ? h('div', { className: 'bae-a-albumhead-fact bae-a-albumhead-gap' }, props.pressing) : null, props.labels ? h('div', { className: 'bae-a-albumhead-fact' }, props.labels) : null));
  }

  function AndroidDownloadControl(props) {
    var s = props.state || 'available';
    function btn(icon, text) { return h('button', { type: 'button', className: 'bae-a-btn bae-a-btn-outlined bae-a-btn-icon' }, h(Glyph, { name: icon, size: 20 }), text); }
    var line = function (t) { return h('span', { className: 'bae-a-fineline' }, t); };
    var body =
      s === 'elsewhere' ? line(props.where || 'On Ana’s Mac') :
      s === 'queued' ? h('div', { className: 'bae-a-dlrow' }, line('Waiting to download'), btn('close', 'Cancel')) :
      s === 'downloading' ? [h(AndroidProgress, { key: 'p', value: Number(props.progress || 0.4) }), h('span', { key: 'b', className: 'bae-a-fine' }, props.bytes || '120 MB of 300 MB'), h('div', { key: 'c' }, btn('close', 'Cancel'))] :
      s === 'failed' ? [h('div', { key: 'e', className: 'bae-a-error' }, props.error || 'Couldn’t reach your storage.'), h('div', { key: 'r', className: 'bae-a-dlrow' }, btn('retry', 'Retry'), btn('close', 'Cancel'))] :
      s === 'downloaded' ? h('div', { className: 'bae-a-dlrow' }, h(Glyph, { name: 'downloaded', size: 16, className: 'bae-muted' }), line('Downloaded'), btn('remove', 'Remove Download')) :
      btn('download', 'Download');
    return h('div', { className: 'bae-a-dl' }, body);
  }


  // ---- Search ----
  function MacSearchCard(props) {
    var style = { position: 'absolute', top: Number(props.top != null ? props.top : 60), right: Number(props.right != null ? props.right : 56), width: 440, zIndex: 6 };
    return h('div', { className: 'bae-m-searchcard', style: style }, props.empty ? h('div', { className: 'bae-m-searchcard-empty' }, props.empty) : props.children);
  }
  function MacSearchSection(props) { return h('div', { className: 'bae-m-searchsection' }, props.title); }
  function MacSearchRow(props) {
    return h('div', { className: cx('bae-m-searchrow', props.hover && 'bae-m-searchrow-hover') },
      h(Artwork, { title: props.art || props.title, size: 44, radius: 6, missing: props.noArt }),
      h('div', { className: 'bae-m-searchrow-text' }, h('div', { className: 'bae-m-searchrow-title' }, props.title), h('div', { className: 'bae-m-searchrow-sub' }, props.subtitle || ' ')),
      props.trailing ? h('span', { className: 'bae-m-searchrow-trail' }, props.trailing) : null);
  }
  function AndroidSearchSection(props) { return h('div', { className: 'bae-a-searchsection' }, props.title); }
  function AndroidSearchRow(props) {
    return h('div', { className: cx('bae-a-searchrow', props.part && 'bae-a-searchrow-part', props.inset && 'bae-a-searchrow-inset') },
      props.noArt ? null : h(Artwork, { title: props.art || props.title, size: 48, radius: 6 }),
      h('div', { className: 'bae-a-searchrow-text' }, h('div', { className: 'bae-a-searchrow-title' }, props.title), props.subtitle ? h('div', { className: 'bae-a-searchrow-sub' }, props.subtitle) : null),
      props.trailing ? h('span', { className: 'bae-a-searchrow-trail' }, props.trailing) : null);
  }

  // ---- Queue and playback (macOS) ----
  function MacQueuePanel(props) {
    return h('aside', { className: 'bae-m-queue' },
      h('div', { className: 'bae-m-queue-head' }, h('h2', null, 'Queue'), h('span', { className: 'bae-m-albumpanel-close', style: { position: 'static' }, 'aria-label': 'Close' }, h(Glyph, { set: 'mac', name: 'close', size: 13 }))),
      props.title ? h('div', { className: 'bae-m-queue-np' },
        h(Artwork, { title: props.album || props.title, size: 56, radius: 6, shadow: 'bar' }),
        h('div', { className: 'bae-m-queue-np-text' },
          h('div', { className: 'bae-m-queue-np-eyebrow' }, 'Now Playing'),
          h('div', { className: 'bae-m-np-title' }, props.title), h('div', { className: 'bae-m-np-sub' }, props.artist),
          h('div', { className: 'bae-m-queue-strip' }, h('div', { className: 'bae-m-queue-strip-bar' }, h('div', { style: { width: (Number(props.position || 0) * 100) + '%' } })), h('span', null, props.elapsed || ''))),
        h('span', { className: 'bae-m-queue-np-play' }, h(Glyph, { set: 'mac', name: props.playing ? 'pause' : 'play', size: 13 }))) : null,
      props.empty ? h('div', { className: 'bae-m-queue-empty' }, h('div', { className: 'bae-m-cu-title' }, 'Queue is empty'), h('div', { className: 'bae-m-cu-desc' }, 'Drag tracks here or play an album'))
        : h('div', { className: 'bae-m-queue-body' }, props.children));
  }
  function MacQueueSection(props) {
    return h('div', { className: 'bae-m-queue-section' },
      h('span', { className: 'bae-m-queue-section-title' }, props.title),
      props.clear ? h('span', { className: 'bae-m-queue-clear' }, 'Clear') : null,
      props.shuffle != null ? h('span', { className: cx('bae-m-queue-shuffle', props.shuffle && 'bae-m-queue-shuffle-on'), 'aria-label': props.shuffle ? 'Turn off shuffle' : 'Shuffle' }, h(Glyph, { set: 'mac', name: 'shuffle', size: 11 })) : null);
  }
  function MacQueuePlaceholderRow() {
    return h('div', { className: 'bae-m-qrow' }, h('div', { className: 'bae-ph', style: { width: 48, height: 48, borderRadius: 6 } }),
      h('div', { className: 'bae-m-qrow-text', style: { gap: 4 } }, h('div', { className: 'bae-ph', style: { width: 140, height: 12 } }), h('div', { className: 'bae-ph', style: { width: 90, height: 10 } }), h('div', { className: 'bae-ph', style: { width: 120, height: 10 } })));
  }
  function MacQueueInsertLine() { return h('div', { className: 'bae-m-queue-insert' }); }

  function MacSidePause(props) {
    var disc = props.kind === 'disc';
    return h('div', { className: 'bae-m-scrim' }, h('div', { className: 'bae-m-sidepause', role: 'dialog' },
      h('div', { className: 'bae-m-sidepause-title' }, (disc ? 'Disc ' : 'Side ') + (props.label || (disc ? '1' : 'A')) + ' ended'),
      h('div', { className: 'bae-muted' }, 'Playback is paused. Press Play to continue.'),
      props.countdown ? h('div', { className: 'bae-m-sidepause-count' }, 'The next ' + (disc ? 'disc' : 'side') + ' starts in ' + props.countdown + ' seconds.') : null,
      h(MacCheckbox, { label: disc ? 'Pause between discs' : 'Pause between sides', on: !props.unchecked }),
      props.unchecked ? h('div', { className: 'bae-muted' }, 'You can turn this back on in Settings › Playback.') : null,
      h('div', { className: 'bae-m-sidepause-buttons' }, h(MacButton, null, 'Close'), h(MacButton, { kind: 'primary' }, 'Play'))));
  }

  function MacCastPopover(props) {
    var devices = String(props.devices || '').split(',').filter(Boolean).map(function (d) { var p = d.trim().split(':'); return { name: p[0], kind: p[1] || 'cast' }; });
    var glyph = { cast: 'cast', airplay: 'airplay', upnp: 'tv' };
    return h('div', { className: 'bae-m-castpop', style: { position: 'absolute', right: Number(props.right != null ? props.right : 120), bottom: Number(props.bottom != null ? props.bottom : 92) } },
      props.connecting ? h('div', { className: 'bae-m-castpop-wait' }, h(MacSpinner, { size: 14 }), 'Connecting to ' + props.connecting + '…') : null,
      props.casting ? h('div', { className: 'bae-m-castpop-casting' }, h(Glyph, { set: 'mac', name: 'cast', size: 15, className: 'bae-accent' }), h('strong', null, 'Casting to ' + props.casting), h(MacButton, { kind: 'borderless' }, 'Disconnect')) : null,
      devices.length ? devices.map(function (d, i) { return h('div', { key: i, className: 'bae-m-castpop-row' }, h(Glyph, { set: 'mac', name: glyph[d.kind] || 'cast', size: 15, className: 'bae-muted' }), h('span', null, d.name), d.name === props.casting ? h(Glyph, { set: 'mac', name: 'check', size: 13, className: 'bae-accent' }) : null); })
        : (props.connecting || props.casting ? null : h('div', { className: 'bae-muted' }, 'No Cast devices found')));
  }

  // ---- Queue and playback (Android) ----
  function AndroidSheet(props) {
    return h('div', { className: 'bae-a-scrim' }, h('div', { className: 'bae-a-sheet', style: props.top != null ? { top: Number(props.top) } : null }, h('div', { className: 'bae-a-sheet-handle' }), props.children));
  }
  function AndroidQueueLabel(props) {
    return h('div', { className: 'bae-a-queuelabel' }, h('span', { className: 'bae-a-eyebrow', style: { paddingTop: 0, flex: 1 } }, props.title),
      props.clear ? h(AndroidButton, { kind: 'text' }, 'Clear') : null,
      props.shuffle != null ? h(AndroidIcon, { icon: 'shuffle', label: props.shuffle ? 'Turn off shuffle' : 'Shuffle', size: 20, className: props.shuffle ? 'bae-a-icon-accent' : 'bae-a-icon-muted' }) : null);
  }
  function AndroidQueueRow(props) {
    return h('div', { className: cx('bae-a-qrow', props.dragging && 'bae-a-qrow-drag') }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
      h('div', { className: 'bae-a-qrow-text' }, h('div', { className: cx('bae-a-track-title', props.current && 'bae-accent') }, props.title), h('div', { className: 'bae-a-track-artist' }, props.artist), props.current ? null : h('div', { className: 'bae-a-track-artist' }, props.album)),
      props.current ? null : [h('span', { key: 'd', className: 'bae-a-track-time', style: { padding: '0 8px' } }, props.duration), h(AndroidIcon, { key: 'x', icon: 'close', label: 'Remove', className: 'bae-a-icon-muted' }), h(Glyph, { key: 'h', name: 'drag', size: 20, className: 'bae-muted' })]);
  }
  function AndroidFullPlayer(props) {
    return h('div', { className: 'bae-a-fullplayer' },
      h(AndroidIcon, { icon: 'down', label: 'Collapse' }),
      h(Artwork, { title: props.album || props.title, radius: 10 }),
      h('div', { className: 'bae-a-fp-title' }, props.title), h('div', { className: 'bae-a-fp-artist' }, props.artist),
      h(AndroidSeekBar, { position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration }),
      h('div', { className: 'bae-a-fp-transport' }, h(AndroidIcon, { icon: 'previous', size: 36, label: 'Previous' }), h(AndroidPlayPause, { playing: props.playing, waiting: props.waiting, size: 48 }), h(AndroidIcon, { icon: 'next', size: 36, label: 'Next' })),
      h('div', { className: 'bae-a-fp-repeat' }, h(AndroidIcon, { icon: 'repeat', label: 'Repeat mode', className: props.repeat ? 'bae-a-icon-accent' : 'bae-a-icon-muted' })),
      h('div', { className: 'bae-a-fp-volume' }, h(AndroidIcon, { icon: 'nowplaying', label: 'Mute', className: 'bae-a-icon-muted' }), h('div', { className: 'bae-a-fp-slider' }, h('div', { style: { width: '70%' } }), h('span', { style: { left: '70%' } }))));
  }
  function AndroidCastRow(props) {
    if (props.waiting) return h('div', { className: 'bae-a-castrow' }, h(AndroidSpinner, { size: 20 }), h('span', { className: 'bae-muted', style: { fontSize: 14 } }, props.waiting));
    if (props.casting) return h('div', { className: 'bae-a-castrow' }, h(Glyph, { name: 'cast', size: 24, className: 'bae-accent' }), h('span', { className: 'bae-a-castrow-name' }, 'Casting to ' + props.casting), h(AndroidButton, { kind: 'text' }, 'Disconnect'));
    var icon = { cast: 'cast', airplay: 'speakerbox', upnp: 'tv' }[props.kind || 'cast'];
    return h('div', { className: 'bae-a-castrow' }, h(Glyph, { name: icon, size: 24, className: 'bae-muted' }), h('span', { className: 'bae-a-castrow-name' }, props.name), props.active ? h(Glyph, { name: 'check', size: 24, className: 'bae-accent' }) : null);
  }
  function AndroidCheckbox(props) {
    return h('div', { className: 'bae-a-checkrow' }, h('span', { className: cx('bae-a-checkbox', props.on && 'bae-a-checkbox-on') }, props.on ? h(Glyph, { name: 'check', size: 16 }) : null), h('span', null, props.label));
  }


  // ---- Composer and work pages (macOS) ----
  // A row of the composer's page or a work's: cover, title, second line. `credit` is the text-only row
  // (a credit or a recording: nothing to open). `part` indents a work under its group's heading work.
  // `top` is the gap above the row, which the page sets per row.
  function MacDetailRow(props) {
    var style = { marginTop: Number(props.top || 0), marginLeft: props.part ? 16 : 0 };
    var text = h('div', { className: 'bae-m-drow-text' }, h('div', { className: 'bae-m-drow-title' }, props.title), h('div', { className: 'bae-m-drow-sub' }, props.subtitle || ' '));
    if (props.credit) return h('div', { className: 'bae-m-drow bae-m-drow-credit', style: style }, text);
    return h('div', { className: cx('bae-m-drow', props.hover && 'bae-m-drow-hover'), style: style },
      h(Artwork, { title: props.art || props.title, size: 44, radius: 6, missing: props.noArt }), text);
  }
  function MacSectionHeader(props) {
    return h('div', { className: 'bae-m-sechead', style: { marginTop: Number(props.top || 0), marginBottom: Number(props.bottom || 0) } }, props.title);
  }
  // A work's detail: its title, then its sections (MacSectionHeader and MacDetailRow) 12pt apart.
  function MacWorkDetail(props) {
    return h('div', { className: 'bae-m-work' }, h('div', { className: 'bae-m-work-title' }, props.title), props.children);
  }
  function MacHairline() { return h('div', { className: 'bae-m-hairline' }); }
  function MacContentUnavailable(props) {
    return h('div', { className: 'bae-m-cu' }, h('div', { className: 'bae-m-cu-title' }, props.title), props.description ? h('div', { className: 'bae-m-cu-desc' }, props.description) : null);
  }

  window.Bae = Object.assign(window.Bae || {}, {
    MacSearchCard: MacSearchCard, MacSearchSection: MacSearchSection, MacSearchRow: MacSearchRow, AndroidSearchSection: AndroidSearchSection, AndroidSearchRow: AndroidSearchRow, MacQueuePanel: MacQueuePanel, MacQueueSection: MacQueueSection, MacQueuePlaceholderRow: MacQueuePlaceholderRow, MacQueueInsertLine: MacQueueInsertLine, MacSidePause: MacSidePause, MacCastPopover: MacCastPopover, AndroidSheet: AndroidSheet, AndroidQueueLabel: AndroidQueueLabel, AndroidQueueRow: AndroidQueueRow, AndroidFullPlayer: AndroidFullPlayer, AndroidCastRow: AndroidCastRow, AndroidCheckbox: AndroidCheckbox,
    Artwork: Artwork, MacMainWindow: MacMainWindow, MacNowPlayingBar: MacNowPlayingBar, MacSeekBar: MacSeekBar, MacPlayPause: MacPlayPause, MacIconButton: MacIconButton, MacLibraryHeading: MacLibraryHeading, MacAlbumGrid: MacAlbumGrid, MacAlbumTile: MacAlbumTile, MacTrackRow: MacTrackRow, MacEyebrow: MacEyebrow, MacBrowseRow: MacBrowseRow, MacDetailHeader: MacDetailHeader, MacQueueRow: MacQueueRow, MacAlbumPanel: MacAlbumPanel, MacTrackColumns: MacTrackColumns, MacBrowseSplit: MacBrowseSplit, MacArtistAlbumTile: MacArtistAlbumTile, MacSortCriteria: MacSortCriteria, MacCheckbox: MacCheckbox, MacArtistGroupHeading: MacArtistGroupHeading, AndroidMenu: AndroidMenu, AndroidMenuItem: AndroidMenuItem,
    AndroidLibraryScreen: AndroidLibraryScreen, AndroidDetailScreen: AndroidDetailScreen, AndroidNowPlayingBar: AndroidNowPlayingBar, AndroidSeekBar: AndroidSeekBar, AndroidPlayPause: AndroidPlayPause, AndroidAlbumGrid: AndroidAlbumGrid, AndroidAlbumTile: AndroidAlbumTile, AndroidArtistHeading: AndroidArtistHeading, AndroidTrackRow: AndroidTrackRow, AndroidEyebrow: AndroidEyebrow, AndroidBrowseRow: AndroidBrowseRow, AndroidAlbumHeader: AndroidAlbumHeader, AndroidDownloadControl: AndroidDownloadControl,
    MacWindow: MacWindow, MacAlert: MacAlert, MacSettingsWindow: MacSettingsWindow, MacSection: MacSection, MacBox: MacBox, MacRow: MacRow, MacFieldRow: MacFieldRow, MacButton: MacButton,
    MacTextField: MacTextField, MacRadio: MacRadio, MacPopUpButton: MacPopUpButton, MacSegmentedControl: MacSegmentedControl, MacDisclosure: MacDisclosure, MacSheet: MacSheet,
    MacSheetFooter: MacSheetFooter, MacProgress: MacProgress, MacSpinner: MacSpinner, MacErrorText: MacErrorText, MacNote: MacNote,
    MacSwitch: MacSwitch, MacAccentPicker: MacAccentPicker, MacMenu: MacMenu, MacMenuItem: MacMenuItem, AndroidAccentPicker: AndroidAccentPicker,
    AndroidScreen: AndroidScreen, AndroidSection: AndroidSection, AndroidCard: AndroidCard,
    AndroidRow: AndroidRow, AndroidButton: AndroidButton, AndroidIconButton: AndroidIconButton, AndroidSwitch: AndroidSwitch,
    AndroidTextField: AndroidTextField, AndroidRadio: AndroidRadio, AndroidDialog: AndroidDialog,
    AndroidDialogActions: AndroidDialogActions, AndroidNote: AndroidNote, AndroidProgress: AndroidProgress, AndroidSpinner: AndroidSpinner,
    StatusChip: StatusChip, Notice: Notice, MacScanCode: MacScanCode, AndroidScanCode: AndroidScanCode,
    MacDetailRow: MacDetailRow, MacSectionHeader: MacSectionHeader, MacWorkDetail: MacWorkDetail, MacHairline: MacHairline, MacContentUnavailable: MacContentUnavailable
  });
})();
