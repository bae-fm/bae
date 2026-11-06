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
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-m-switch', on && 'bae-m-switch-on', props.off && 'bae-m-switch-off'), disabled: !!props.off, onClick: function () { s[1](!on); } },
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
    if (!props.title && !props.error) return null;
    return h('div', { className: 'bae-a-npbar' },
      h('div', { className: 'bae-a-np-row' },
        props.error ? h('div', { className: 'bae-a-np-info bae-a-np-error' }, props.error) : h('div', { className: 'bae-a-np-info' }, h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
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
      h(AndroidNowPlayingBar, { title: props.trackTitle, artist: props.trackArtist, album: props.trackAlbum, playing: props.playing, waiting: props.waiting, position: props.position, held: props.held, elapsed: props.elapsed, duration: props.duration, casting: props.casting, repeat: props.repeat, error: props.barError }));
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

  // ==== part: storage ====

  // ---- Stroke symbols for the parts below (SF Symbols and Material icons, drawn as lines) ----
  var SYM = {
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    pin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9',
    unpin: 'M9 3h6l-1 6 3 3H7l3-3zM12 12v9M4 4l16 16',
    cloud: 'M7 18a4 4 0 0 1-.5-8A6 6 0 0 1 18 9a4.5 4.5 0 0 1 0 9z',
    up: 'M12 20V5M6 11l6-6 6 6',
    down: 'M12 4v15M6 13l6 6 6-6',
    pause: 'M8 5v14M16 5v14',
    play: 'M7 4l13 8-13 8z',
    sidebar: 'M3 5h18v14H3zM15 5v14',
    clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2',
    xcircle: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9 9l6 6M15 9l-6 6',
    copy: 'M8 8h12v12H8zM4 16V4h12',
    left: 'M15 5l-7 7 7 7',
    right: 'M9 5l7 7-7 7',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5',
    photo: 'M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15 9h.01',
    globe: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18',
    sync: 'M4 12a8 8 0 0 1 14-5.3M20 4v4h-4M20 12a8 8 0 0 1-14 5.3M4 20v-4h4',
    octagon: 'M8 3h8l5 5v8l-5 5H8l-5-5V8zM12 8v5M12 16h.01',
    warning: 'M12 3l10 18H2zM12 10v5M12 18h.01',
    moveToCloud: 'M12 4v12M7 11l5 5 5-5M5 20h14',
    makeLocal: 'M14 4h6v6M20 4l-9 9M18 14v6H4V6h6',
    export: 'M12 15V3M7 8l5-5 5 5M5 12v8h14v-8',
    saveAs: 'M12 3v12M7 10l5 5 5-5M5 12v8h14v-8',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
    expand: 'M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7',
    close: 'M6 6l12 12M18 6L6 18',
    back: 'M20 12H5M11 6l-6 6 6 6',
    more: 'M12 5h.01M12 12h.01M12 19h.01',
    check: 'M5 12l5 5 9-10',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
    edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4'
  };
  function Sym(props) {
    var s = Number(props.size || 14);
    return h('svg', { width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true, className: cx('bae-sym', props.className), fill: 'none', stroke: 'currentColor', strokeWidth: props.weight || 2, strokeLinecap: 'round', strokeLinejoin: 'round', style: props.style }, h('path', { d: SYM[props.name] || '' }));
  }
  function list(s) { return String(s || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean); }

  // ---- Storage Manager (macOS) ----
  // The window: title, Filter field, All / Local / Cloud, the Inspector toggle, then the release list and the inspector.
  function MacStorageWindow(props) {
    var k = kids(props.children);
    return h('div', { className: 'bae-m-window bae-m-storewin', style: { width: Number(props.width || 1100), height: Number(props.height || 660) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      h('header', { className: 'bae-m-storewin-bar' },
        h('span', { className: 'bae-m-storewin-title' }, 'Storage Manager'),
        h('span', { style: { flex: 1 } }),
        h(MacSegmentedControl, { options: 'All, Local, Cloud', selected: props.filter || 'All' }),
        h('span', { style: { flex: 1 } }),
        h('span', { className: 'bae-m-storewin-filter' }, h(Glyph, { set: 'mac', name: 'search', size: 12 }), h('span', { className: props.search ? null : 'bae-m-search-prompt' }, props.search || 'Filter')),
        h('span', { className: cx('bae-m-iconbtn', props.inspector && 'bae-m-iconbtn-on'), title: 'Inspector' }, h(Sym, { name: 'sidebar', size: 15 }))),
      h('div', { className: 'bae-m-storewin-body' },
        h('div', { className: 'bae-m-storewin-list' }, k[0]),
        props.inspector ? h('div', { className: 'bae-m-storewin-inspector' }, k.slice(1)) : null));
  }
  // An AppKit table: columns "Title:width[:right][:up|:down]", rows as MacTableRow.
  function cols(s) { return list(s).map(function (c) { var p = c.split(':'); return { title: p[0], w: Number(p[1] || 100), right: p.indexOf('right') > 1, sort: p.indexOf('up') > 1 ? 'up' : p.indexOf('down') > 1 ? 'down' : null }; }); }
  function MacTable(props) {
    var cs = cols(props.columns);
    var tmpl = cs.map(function (c, i) { return i === 0 ? 'minmax(' + c.w + 'px, 1fr)' : c.w + 'px'; }).join(' ');
    return h('div', { className: 'bae-m-table', style: { '--bae-cols': tmpl } },
      h('div', { className: 'bae-m-table-head' }, cs.map(function (c, i) { return h('span', { key: i, className: cx('bae-m-table-th', c.right && 'bae-m-table-right') }, c.title, c.sort ? h(Glyph, { set: 'mac', name: 'down', size: 8, className: 'bae-muted' }) : null); })),
      h('div', { className: 'bae-m-table-body' }, kids(props.children).map(function (r, i) { return h('div', { key: i, className: cx('bae-m-table-tr', i % 2 && 'bae-m-table-alt') }, r); })),
      props.empty ? h('div', { className: 'bae-m-table-empty' }, h('div', { className: 'bae-m-cu-title' }, props.empty)) : null);
  }
  function MacTableRow(props) {
    if (props.placeholder) return h('div', { className: 'bae-m-table-row' }, h('span', { className: 'bae-m-table-td' }, h('span', { className: 'bae-ph', style: { width: 180, height: 14, borderRadius: 3 } })));
    return h('div', { className: cx('bae-m-table-row', props.selected && 'bae-m-table-selected') }, kids(props.children).map(function (c, i) { return h('span', { key: i, className: 'bae-m-table-td' }, c); }));
  }
  function MacReleaseCell(props) { return h('span', { className: 'bae-m-relcell' }, h(Artwork, { title: props.title, size: 24, radius: 4 }), h('span', { className: 'bae-ellipsis' }, props.title)); }
  // The Storage column: Local, Pinned, Cloud; or a transfer: Uploading and Pinning fill as they go.
  function MacStorageChip(props) {
    var s = props.state || 'cloud';
    if (s === 'uploading' || s === 'pinning' || s === 'detaching') {
      var label = s === 'uploading' ? 'Uploading' : s === 'pinning' ? 'Pinning' : 'Detaching Audio…';
      return h('span', { className: cx('bae-m-progchip', s === 'uploading' ? 'bae-m-progchip-up' : 'bae-m-progchip-pin') }, h('span', { className: 'bae-m-progchip-fill', style: { width: (Number(props.progress || 0.4) * 100) + '%' } }), h('span', { className: 'bae-m-progchip-label' }, label));
    }
    if (s === 'transfer') return h(StatusChip, { tone: 'activity' }, props.label || 'Pinning for offline');
    var icon = { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s];
    return h(StatusChip, { tone: 'neutral' }, h(Sym, { name: icon, size: 10 }), { local: 'Local', pinned: 'Pinned', cloud: 'Cloud' }[s]);
  }
  // Under the list: how many releases, the total, then one rate and Pause per queue with work.
  function MacStorageFooter(props) {
    var q = function (kind, rate, label) { return h('span', { className: 'bae-m-storefoot-q' }, h(Sym, { name: kind === 'exports' ? 'export' : kind === 'uploads' ? 'up' : 'down', size: 11, className: kind === 'uploads' ? 'bae-activity' : kind === 'pins' ? 'bae-pinning' : null }), rate ? h('span', { className: 'bae-num' }, rate) : null, h('span', { title: label }, h(Sym, { name: 'pause', size: 10 }))); };
    return h('div', { className: 'bae-m-storefoot' },
      h('span', null, props.releases || '8 releases', props.total ? ' · Total: ' + props.total : ''),
      h('span', { style: { flex: 1 } }),
      props.uploads ? q('uploads', props.uploads, 'Pause uploads') : null,
      props.pins ? q('pins', props.pins, 'Pause pinning') : null,
      props.exports ? q('exports', props.exports, 'Pause exports') : null);
  }
  // A release's upload or pin in the inspector: the counts line with Cancel, then bytes, rate and a bar.
  function MacTransferSummary(props) {
    var kind = props.kind || 'upload';
    var parts = list(props.counts);
    return h('div', { className: 'bae-m-tsum' },
      h('div', { className: 'bae-m-tsum-line' },
        h('span', { className: 'bae-m-tsum-counts' }, parts.map(function (p, i) { return h(React.Fragment, { key: i }, i ? h('span', { className: 'bae-muted' }, ' · ') : null, h('span', { className: i === 0 ? cx('bae-strong', kind === 'upload' ? 'bae-activity' : 'bae-pinning') : i === parts.length - 1 ? 'bae-muted' : null }, p)); })),
        h('span', { style: { flex: 1 } }), props.cancel === false ? null : h(MacButton, { size: 'small' }, 'Cancel')),
      props.bytes ? h(MacTransferReadout, { kind: kind, bytes: props.bytes, rate: props.rate, progress: props.progress }) : null);
  }
  function MacTransferReadout(props) {
    return h('div', { className: 'bae-m-readout' },
      h('div', { className: 'bae-m-readout-line bae-num' }, h('span', null, props.bytes), h('span', null, props.rate)),
      h('div', { className: 'bae-m-track' }, h('div', { className: props.kind === 'pin' ? 'bae-pinning-bg' : 'bae-activity-bg', style: { width: (Number(props.progress || 0.3) * 100) + '%' } })));
  }
  // One file in the inspector: a mark for queued (dashed), sending or pinning (a ring filling), its name and size.
  function MacFileRow(props) {
    var s = props.state || 'rest';
    var p = Number(props.progress || 0.4), C = 2 * Math.PI * 6;
    var mark = s === 'rest' ? h('span', { style: { width: 16 } }) :
      s === 'queued' ? h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: 'bae-muted' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 1.75, strokeDasharray: '2 2' })) :
      h('svg', { width: 16, height: 16, viewBox: '0 0 16 16', className: s === 'pinning' ? 'bae-pinning' : 'bae-activity' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeDasharray: (C * p) + ' ' + C, transform: 'rotate(-90 8 8)' }));
    return h('div', { className: cx('bae-m-filerow', props.selected && 'bae-m-filerow-selected') }, mark,
      h('span', { className: cx('bae-ellipsis', s === 'queued' && 'bae-muted', s === 'sending' && 'bae-activity', s === 'pinning' && 'bae-pinning'), style: { flex: 1 } }, props.name),
      h('span', { className: 'bae-muted bae-num' }, props.size));
  }
  function MacFileDetails(props) {
    var row = function (l, v) { return v ? [h('span', { key: l + 'l', className: 'bae-muted' }, l), h('span', { key: l + 'v', className: l === 'Size' ? 'bae-num' : null }, v)] : null; };
    return h('div', { className: 'bae-m-filedet' },
      h('div', { className: 'bae-m-filedet-head' }, h('span', { className: 'bae-ellipsis' }, props.name), h('span', { className: 'bae-muted', title: 'Close file details' }, h(Sym, { name: 'xcircle', size: 15 }))),
      h('div', { className: 'bae-m-filedet-grid' }, row('Folder', props.folder), row('Kind', props.kind), row('Format', props.format), row('Size', props.size)));
  }
  // BaeKit's ErrorDetailDisclosure: the message in its tone, the fault with Copy details, then Details.
  function MacErrorDisclosure(props) {
    var tone = props.tone || 'danger';
    return h('div', { className: 'bae-m-errdis' },
      h('div', { className: cx('bae-m-errdis-msg', 'bae-tone-' + tone) }, props.icon === false ? null : h(Sym, { name: tone === 'warning' ? 'warning' : 'octagon', size: 13 }), h('span', null, props.message)),
      props.fault ? h('div', { className: 'bae-m-errdis-fault' }, h('span', { className: 'bae-mono bae-muted' }, props.fault), h('span', { className: 'bae-m-errdis-copy', title: 'Copy details' }, h(Sym, { name: 'copy', size: 11 }))) : null,
      props.fault ? h('div', { className: 'bae-m-errdis-more' }, h(Sym, { name: props.open ? 'down' : 'right', size: 9 }), 'Details') : null,
      props.open && props.excerpt ? h('div', { className: 'bae-mono bae-muted', style: { whiteSpace: 'pre-wrap' } }, props.excerpt) : null);
  }
  // A popover hanging from a title-bar button, its arrow pointing up at it.
  function MacPopover(props) {
    return h('div', { className: 'bae-m-popover', style: { width: Number(props.width || 360), top: props.top != null ? Number(props.top) : null, right: props.right != null ? Number(props.right) : null } }, h('span', { className: 'bae-m-popover-arrow' }), props.children);
  }
  // BaeKit's SyncActivityList: Needs Attention, Transfers, or up to date; then the store's error and Retry Now.
  function MacSyncActivity(props) {
    var sec = function (title, body) { return h(MacSection, { title: title }, h(MacBox, null, body)); };
    var label = function (icon, text, sub) { return h('div', { className: 'bae-m-actrow' }, h(Sym, { name: icon, size: 15, className: 'bae-accent' }), h('div', null, h('div', null, text), sub ? h('div', { className: 'bae-m-actrow-sub bae-num' }, sub) : null)); };
    var out = [];
    if (props.attention) out.push(h(React.Fragment, { key: 'a' }, sec('Needs Attention', [
      h('div', { key: 'f', className: 'bae-m-actpad' }, h(MacErrorDisclosure, { tone: 'warning', message: props.attention, fault: props.fault })),
      props.record ? h('div', { key: 'r', className: 'bae-m-actpad' }, h('div', { className: 'bae-strong' }, props.record), h(MacErrorDisclosure, { tone: 'neutral', icon: false, message: props.reason })) : null,
      h('div', { key: 'b', className: 'bae-m-actpad' }, h(MacButton, null, 'Open Sync Settings'))].filter(Boolean))));
    if (props.syncing || props.uploads || props.downloads) out.push(h(React.Fragment, { key: 't' }, sec('Transfers', [
      props.syncing ? h('div', { key: 's', className: 'bae-m-actpad' }, label('sync', 'Syncing library changes…')) : null,
      props.uploads ? h('div', { key: 'u', className: 'bae-m-actpad' }, label('up', props.uploads, props.uploadBytes)) : null,
      props.downloads ? h('div', { key: 'd', className: 'bae-m-actpad' }, label('down', props.downloads)) : null].filter(Boolean))));
    if (!out.length) out.push(h(React.Fragment, { key: 'i' }, sec(null, h('div', { className: 'bae-m-actpad bae-muted' }, 'Everything is up to date.'))));
    if (props.error) out.push(h(React.Fragment, { key: 'e' }, sec(null, [h('div', { key: 'e', className: 'bae-m-actpad' }, h(MacErrorDisclosure, { message: props.error, fault: props.errorFault })), props.retry ? h('div', { key: 'r', className: 'bae-m-actpad' }, h(MacButton, null, 'Retry Now')) : null].filter(Boolean))));
    return h('div', { className: 'bae-m-actlist' }, out);
  }

  // ---- Android: sync activity and Downloads ----
  function AndroidErrorDisclosure(props) {
    var tone = props.tone || 'danger';
    return h('div', { className: 'bae-a-errdis' },
      h('div', { className: cx('bae-a-errdis-msg', 'bae-tone-' + tone) }, props.message),
      props.fault ? h('div', { className: 'bae-a-errdis-fault' }, h('span', { className: 'bae-mono bae-muted' }, props.fault), h(AndroidIconButton, { icon: 'copy', label: 'Copy details' })) : null);
  }
  // SyncActivity.kt's sheet: the title, Needs Attention, the transfers (the downloads line opens Downloads), or up to date.
  function AndroidSyncActivity(props) {
    var t = function (s, c) { return h('div', { className: c || 'bae-a-act-line' }, s); };
    return h('div', { className: 'bae-a-act' },
      h('div', { className: 'bae-a-act-title' }, 'Sync Activity'),
      props.attention ? h('div', { className: 'bae-a-act-block' }, h('div', { className: 'bae-strong', style: { paddingTop: 4 } }, 'Needs Attention'), h(AndroidErrorDisclosure, { tone: 'warning', message: props.attention, fault: props.fault }),
        props.record ? h('div', null, h('div', { className: 'bae-strong' }, props.record), h('div', { className: 'bae-muted' }, props.reason)) : null,
        h(AndroidButton, { kind: 'text', alignTo: 'text' }, 'Open Sync Settings'), h('div', { className: 'bae-divider' })) : null,
      props.syncing ? t('Syncing library changes…') : null,
      props.uploads ? h('div', null, t(props.uploads), props.uploadBytes ? t(props.uploadBytes, 'bae-a-act-sub') : null) : null,
      props.downloads ? h('div', { className: 'bae-a-act-tap' }, t(props.downloads)) : null,
      !(props.attention || props.syncing || props.uploads || props.downloads) ? t('Everything is up to date.', 'bae-a-act-sub') : null,
      props.error ? h(AndroidErrorDisclosure, { message: props.error, fault: props.errorFault }) : null,
      props.retry ? h(AndroidButton, { kind: 'outlined' }, 'Retry Now') : null);
  }
  function AndroidSegmentedButtons(props) {
    var o = list(props.options);
    return h('div', { className: 'bae-a-seg', role: 'radiogroup' }, o.map(function (x) { var on = x === String(props.selected); return h('span', { key: x, role: 'radio', 'aria-checked': on, className: cx('bae-a-seg-btn', on && 'bae-a-seg-on') }, on ? h(Glyph, { name: 'check', size: 18 }) : null, x); }));
  }
  // The Downloads screen's bar: Back, the title, Retry and Pause or Resume.
  function AndroidDownloadsBar(props) {
    return h('header', { className: 'bae-a-dlbar' }, h(AndroidIconButton, { icon: 'back', label: 'Back' }), h('span', { className: 'bae-a-dlbar-title' }, 'Downloads'),
      h(AndroidButton, { kind: 'text', off: !props.failed }, 'Retry'), h(AndroidButton, { kind: 'text', off: !!props.idle }, props.paused ? 'Resume' : 'Pause'));
  }
  function AndroidDownloadRow(props) {
    var s = props.state || 'queued';
    return h('div', { className: 'bae-a-dlrow2' },
      h('div', { className: 'bae-a-dlrow2-text' },
        h('div', { className: 'bae-a-browse-name' }, props.title || 'Release no longer in library'),
        h('div', { className: 'bae-a-browse-count' }, props.detail),
        s === 'queued' ? h('div', { className: 'bae-a-browse-count' }, 'Waiting to download') :
        s === 'active' ? h('div', null, h('div', { className: 'bae-a-dlbar-track' }, h('div', { style: { width: (Number(props.progress || 0.4) * 100) + '%' } })), h('div', { className: 'bae-a-fine bae-muted' }, props.bytes || '120 MB of 300 MB')) :
        h('div', { className: 'bae-a-error' }, props.error || 'Couldn’t reach your storage.')),
      h(AndroidIcon, { icon: 'close', label: 'Cancel', className: 'bae-a-icon-muted' }));
  }

  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack' }, h('span', { style: { flex: 1 } }, props.message), props.action ? h('span', { className: 'bae-a-snack-action' }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 20 })));
  }
  // The danger banner above the album grid when a later page failed.
  function AndroidErrorBanner(props) {
    return h('div', { className: 'bae-a-errbanner' }, h(Glyph, { name: 'warning', size: 20, className: 'bae-a-errbanner-icon' }), h('div', { style: { flex: 1 } }, h(AndroidErrorDisclosure, { tone: 'neutral', message: props.message, fault: props.fault })), h(AndroidButton, { kind: 'text' }, 'Retry'));
  }
  // macOS's notice at the bottom of the main window: the step's title, the disclosure, the advice. fill is the launch-problem one.
  function MacWindowNotice(props) {
    return h('div', { className: cx('bae-m-winnotice', props.fill && 'bae-m-winnotice-fill') }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h(MacErrorDisclosure, { message: props.message, fault: props.fault, icon: props.fill ? false : undefined }), props.advice ? h('div', { className: 'bae-muted' }, props.advice) : null);
  }
  // BaeKit's LoadFailureView: the disclosure and Retry, centred in the view it replaces.
  function MacLoadFailure(props) {
    return h('div', { className: 'bae-m-loadfail' }, h(MacErrorDisclosure, { message: props.message, fault: props.fault }), h(MacButton, null, 'Retry'));
  }

  // ==== part: import ====
  // ---- Import (macOS) ----
  // The left column: Found / Imported / Skipped with counts, the filter row with its indicators, then the rows.
  function MacImportSidebar(props) {
    var tab = props.tab || 'Found';
    var counts = { Found: props.found || '0', Imported: props.imported || '0', Skipped: props.skipped || '0' };
    return h('div', { className: 'bae-m-isb' },
      h('div', { className: 'bae-m-isb-tabs' }, ['Found', 'Imported', 'Skipped'].map(function (t) { var on = t === tab; return h('span', { key: t, className: cx('bae-m-isb-tab', on && 'bae-m-isb-tab-on') }, h('span', null, t), h(StatusChip, { tone: on ? 'accent' : 'neutral' }, counts[t])); })),
      h('div', { className: 'bae-divider' }),
      h('div', { className: 'bae-m-isb-filter' },
        h(Glyph, { set: 'mac', name: 'search', size: 12, className: 'bae-m-tertiary' }), h('span', { className: props.filter ? null : 'bae-m-tertiary', style: { flex: 1 } }, props.filter || 'Filter…'),
        props.narrowed ? h('span', { className: 'bae-muted bae-num', style: { fontSize: 11.5 } }, props.narrowed) : null,
        props.identifying ? h('span', { className: 'bae-m-isb-ind', title: 'Identifying' }, h('svg', { width: 14, height: 14, viewBox: '0 0 16 16' }, h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--hairline-strong)', strokeWidth: 2 }), h('circle', { cx: 8, cy: 8, r: 6, fill: 'none', stroke: 'var(--accent)', strokeWidth: 2, strokeDasharray: (Number(props.identifying) * 37.7) + ' 37.7', transform: 'rotate(-90 8 8)' }))) : null,
        props.importing ? h('span', { className: 'bae-m-isb-ind bae-muted', title: 'Importing…' }, h(Sym, { name: 'saveAs', size: 14 })) : null,
        props.scanning ? h('span', { className: 'bae-m-isb-ind', title: 'Scanning folders' }, h(MacSpinner, { size: 12 })) : null,
        h('span', { className: 'bae-m-isb-ind bae-muted', title: 'Sorting, Filtering and Watched Folders' }, h(Glyph, { set: 'mac', name: 'ellipsis', size: 15 }), props.filtered ? h('span', { className: 'bae-m-isb-dot' }) : null)),
      h('div', { className: 'bae-divider' }),
      props.empty ? h('div', { className: 'bae-m-isb-empty' }, h('div', { className: 'bae-m-cu-title' }, props.empty)) : h('div', { className: 'bae-m-isb-list' }, props.children));
  }
  function MacImportGroupHeader(props) {
    return h('div', { className: 'bae-m-igh' }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: props.closed ? { transform: 'rotate(-90deg)' } : null }), h('span', { className: 'bae-ellipsis' }, props.name));
  }
  // One candidate: cover, title over artist (or the folder's name), the state line, the trailing badge or wait mark.
  function MacImportRow(props) {
    var trail = props.badge ? h(StatusChip, { tone: props.badgeTone || 'warning' }, props.badge)
      : props.state === 'identifyQueued' || props.state === 'importQueued' ? h('span', { className: 'bae-muted', title: props.state === 'importQueued' ? 'Waiting to import' : 'Waiting to be identified' }, h(Sym, { name: 'clock', size: 12 }))
      : props.state === 'identifying' ? h(MacSpinner, { size: 12 }) : null;
    var line1 = props.folder ? h('div', { className: 'bae-m-irow-folder' }, h(Sym, { name: 'folder', size: 13, className: 'bae-muted' }), h('span', { className: 'bae-mono bae-ellipsis', style: { fontSize: 12 } }, props.folder))
      : h('div', { className: 'bae-m-irow-t' }, h('span', { className: cx('bae-ellipsis', !props.title && 'bae-muted') }, props.title || 'Album title'), h('span', { style: { opacity: props.record ? 1 : 0 } }, h(Sym, { name: 'makeLocal', size: 10, className: 'bae-muted' })));
    var line2 = props.state === 'importing' ? h('div', { className: 'bae-m-irow-step' }, h('span', null, props.step || 'Reading files...'), h('div', { className: 'bae-m-track', style: { height: 3 } }, h('div', { className: 'bae-accent-bg', style: { width: (Number(props.progress || 0.4) * 100) + '%' } })))
      : props.state === 'uploading' ? h('div', { className: 'bae-m-irow-step' }, h('span', null, 'Uploading'), h('div', { className: 'bae-m-track', style: { height: 3 } }, h('div', { className: 'bae-activity-bg', style: { width: (Number(props.progress || 0.4) * 100) + '%' } })))
      : props.error ? h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.error)
      : props.folder ? null : h('div', { className: 'bae-m-irow-sub bae-ellipsis' }, props.artist);
    return h('div', { className: cx('bae-m-irow', props.grouped && 'bae-m-irow-grouped', props.selected && 'bae-m-irow-selected') },
      props.grouped ? h('span', { className: 'bae-m-irow-rail' }) : null,
      h(Artwork, { title: props.cover || props.title || props.folder, size: 44, radius: 6, missing: props.noCover }),
      h('div', { className: 'bae-m-irow-text' }, line1, line2), h('span', { style: { flex: 'none', marginLeft: 'auto' } }, trail));
  }
  // Above the right pane: the tab chip, Reveal in Finder, the folder's name.
  function MacImportFolderLine(props) {
    return h('div', { className: 'bae-m-ifl' }, h(StatusChip, { tone: 'accent' }, props.tab || 'Found'), h(Sym, { name: 'right', size: 10, className: 'bae-m-tertiary' }), h(Sym, { name: 'folder', size: 15, className: 'bae-muted' }), h('span', { className: 'bae-ellipsis bae-strong', style: { fontSize: 15 } }, props.folder));
  }
  // The draft card: Identify with Automatic, Search and ⋯; the folder checks, Cloud/Pinned and Import; the cover well and identity; the Release grid; the linked records.
  function MacImportDraft(props) {
    var rows = list(props.fields || 'Year:1992,Media:CD,Label:Harbor Records — HR-1042,Country:—,Barcode:—,Status:Official,Packaging:—,Details:—').map(function (r) { var i = r.indexOf(':'); return [r.slice(0, i), r.slice(i + 1)]; });
    return h('div', { className: 'bae-m-card bae-m-idraft' },
      h('div', { className: 'bae-m-idraft-actions' },
        h('span', { className: 'bae-strong bae-muted' }, 'Identify'), h(MacButton, null, 'Automatic'), h(MacButton, null, 'Search'), props.editor ? null : h(MacButton, null, '⋯'),
        h('span', { style: { flex: 1 } }),
        props.check ? h('span', { className: 'bae-m-idraft-warn' }, props.check) : null,
        props.cloud ? h(MacCheckbox, { label: 'Cloud', on: true }) : null, props.cloud ? h(MacCheckbox, { label: 'Pinned' }) : null,
        props.editor ? [props.changed ? h(MacButton, { key: 'c' }, 'Cancel') : null, h(MacButton, { key: 's', kind: 'primary' }, 'Save')] : props.blank ? null : h(MacButton, { kind: 'primary' }, props.retry ? 'Retry Import' : 'Import')),
      h('div', { className: 'bae-m-idraft-head' },
        props.cover ? h(Artwork, { title: props.cover, size: 132, radius: 6 }) : h('div', { className: 'bae-m-idraft-well' }, h('div', { className: 'bae-strong' }, 'Add artwork'), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, 'Drag an image here, or click to choose')),
        h('div', { className: 'bae-m-idraft-id' },
          h('div', { className: cx('bae-m-idraft-title', !props.title && 'bae-m-tertiary') }, props.title || 'Album title'),
          h('div', { className: 'bae-muted', style: { display: 'flex', gap: 6, alignItems: 'center' } }, props.artist || 'Album artist', props.artistChip ? h(StatusChip, { tone: props.artistChip === 'New' ? 'accent' : 'neutral' }, props.artistChip) : null),
          h('div', { className: 'bae-m-tertiary' }, props.year || 'Album year'),
          h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.audio || 'FLAC · 16-bit · 44.1 kHz · 10 files'))),
      h('div', { className: 'bae-m-formhead' }, 'Release'),
      h('div', { className: 'bae-m-idraft-grid' }, rows.map(function (r, i) { return [h('span', { key: 'l' + i, className: 'bae-muted' }, r[0]), h('span', { key: 'v' + i, className: r[1] === '—' ? 'bae-m-tertiary' : null }, r[1])]; })),
      props.records ? h('div', { className: 'bae-m-idraft-records' }, list(props.records).map(function (s) { return h('span', { key: s, className: 'bae-m-reclink' }, s, h(Sym, { name: 'makeLocal', size: 9 })); }), h('span', { style: { flex: 1 } }), h('span', { className: 'bae-accent' }, 'Unlink')) : null);
  }
  // The mapping table: Source | Track | Title | Artist | Length, a side's eyebrow, CUE captions and rows.
  function MacImportTable(props) {
    return h('div', { className: 'bae-m-itable' }, h('div', { className: 'bae-m-itable-row bae-m-itable-head' }, ['Source', 'Track', 'Title', 'Artist', 'Length'].map(function (t) { return h('span', { key: t, className: 'bae-m-eyebrowtxt' }, t); })), props.children);
  }
  function MacImportTrackRow(props) {
    return h('div', { className: cx('bae-m-itable-row', 'bae-m-itable-track', props.playing && 'bae-m-itable-playing') },
      h('span', { className: 'bae-m-isrc' }, h('span', { className: props.playing ? 'bae-accent' : 'bae-muted' }, h(Sym, { name: props.playing ? 'pause' : 'play', size: 10 })), h('span', { className: props.entry ? 'bae-ellipsis' : 'bae-mono bae-ellipsis' }, props.file), props.evidence ? h(StatusChip, { tone: 'accent' }, props.evidence) : null),
      h('span', { className: 'bae-num' }, props.position),
      h('span', { className: cx('bae-ellipsis', !props.title && 'bae-m-tertiary') }, props.title || 'Untitled'),
      h('span', { className: cx('bae-ellipsis', props.inherited && 'bae-m-tertiary') }, props.artist || 'Album artist'),
      h('span', { className: cx('bae-num', props.mismatch && 'bae-warning'), style: { textAlign: 'right' }, title: props.mismatch ? 'Lengths differ' : null }, props.length));
  }
  function MacImportSideHeader(props) { return h('div', { className: 'bae-m-eyebrowtxt', style: { padding: '12px 0 4px' } }, props.title); }
  function MacImportSheetCaption(props) {
    return h('div', { className: 'bae-m-icue' }, h('span', { className: 'bae-m-icue-disc' }, h('span', { className: 'bae-strong' }, props.disc || 'Disc 1'), h(Glyph, { set: 'mac', name: 'down', size: 8 })), h(StatusChip, { tone: 'accent' }, 'CUE'), h('span', { className: 'bae-mono' }, props.sheet), h('span', { className: 'bae-m-tertiary' }, '→'), h('span', { className: cx('bae-mono', props.unbound ? 'bae-warning' : 'bae-muted') }, props.bound || 'Choose audio…'));
  }
  // Find online: Back and Identify, the Discogs bar, then Automatic and Search, one open at a time.
  function MacFindOnline(props) {
    var open = props.open || 'Automatic';
    var head = function (t, status) { return h('div', { className: cx('bae-m-fo-sec', open !== t && props.dimmed === t && 'bae-m-fo-dim') }, h(Glyph, { set: 'mac', name: 'down', size: 9, className: 'bae-m-tertiary', style: open === t ? null : { transform: 'rotate(-90deg)' } }), h('span', { className: 'bae-m-eyebrowtxt' }, t), status); };
    var k = kids(props.children);
    return h('div', { className: 'bae-m-card bae-m-fo' },
      h('div', { className: 'bae-m-fo-head' }, h('span', { className: 'bae-accent', style: { display: 'flex', alignItems: 'center', gap: 4 } }, h(Sym, { name: 'left', size: 12 }), 'Back'), h('span', { className: 'bae-strong', style: { position: 'absolute', left: 0, right: 0, textAlign: 'center', pointerEvents: 'none' } }, 'Identify')),
      h('div', { className: 'bae-divider' }),
      props.discogsBar ? h('div', { style: { padding: 12 } }, h(Notice, { tone: 'info' }, h('div', { style: { display: 'flex', gap: 10 } }, h('div', { style: { flex: 1 } }, h('div', { className: 'bae-strong' }, 'Add Discogs to find more pressings'), h('div', { className: 'bae-muted' }, 'Many regional releases, reissues and vinyl pressings are only on Discogs. Tokens are free: create one in your Discogs account settings.')), h(MacButton, { kind: 'primary', size: 'small' }, 'Open Settings'), h(Sym, { name: 'close', size: 11 })))) : null,
      head('Automatic', props.autoStatus === 'working' ? h(MacSpinner, { size: 11 }) : props.autoStatus === 'failed' ? h(Sym, { name: 'warning', size: 12, className: 'bae-warning' }) : null),
      open === 'Automatic' ? h('div', { className: 'bae-m-fo-body' }, k) : null,
      h('div', { className: 'bae-divider' }),
      head('Search', null),
      open === 'Search' ? h('div', { className: 'bae-m-fo-body' }, k) : null);
  }
  // The identifier chips: "Label:value[:state]" with state filled, outlined, struck or dash; capsules "Source=count".
  function MacIdentifierBand(props) {
    return h('div', { className: 'bae-m-band' }, list(props.chips).map(function (c, i) {
      var p = c.split('|'); var lab = p[0], val = p[1] || '', st = p[2] || 'filled', cap = p[3];
      return h('span', { key: i, className: cx('bae-m-idchip', 'bae-m-idchip-' + st) }, h('span', { className: 'bae-m-tertiary', style: { fontSize: 10.5, fontWeight: 600 } }, lab), st === 'dash' ? h('span', { className: 'bae-m-idchip-dash' }) : h('span', { className: 'bae-mono' }, val),
        cap ? cap.split('+').map(function (cc, j) { var q = cc.split('='); return h('span', { key: j, className: 'bae-m-capsule' }, q[0], q[1] === '…' ? h('span', { className: 'bae-m-tertiary' }, 'Searching…') : h(StatusChip, { tone: Number(q[1]) > 0 ? 'success' : 'neutral' }, q[1])); }) : null);
    }));
  }
  function MacReleaseCard(props) {
    return h('div', { className: 'bae-m-rcard' },
      h('div', { className: 'bae-m-rcard-head' }, h(Artwork, { title: props.title, size: 44, radius: 6 }), h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h('div', { className: 'bae-muted' }, props.artist + (props.label ? ' · ' + props.label : ''))), list(props.sources || 'Discogs,MusicBrainz').map(function (s) { return h('span', { key: s, className: 'bae-m-reclink' }, s, h(Sym, { name: 'makeLocal', size: 9 })); })),
      h('div', { className: 'bae-m-rcard-rows' }, props.children));
  }
  function MacPressingRow(props) {
    return h('div', { className: cx('bae-m-prow', props.selected && 'bae-m-prow-selected'), style: props.inLibrary ? { opacity: 0.55 } : null },
      h('div', { style: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2 } },
        h('div', { style: { display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' } }, props.year ? h('span', { className: 'bae-strong bae-num' }, props.year) : h('span', { className: 'bae-m-tertiary' }, 'Year unknown'), h('span', { className: 'bae-muted' }, props.labels), props.catalog ? h(StatusChip, null, props.catalog) : null, list(props.badges).map(function (b) { return h(StatusChip, { key: b, tone: 'accent' }, b); })),
        props.summary ? h('div', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.summary) : null),
      h('span', { style: { display: 'flex', alignItems: 'center', gap: 4, opacity: props.inLibrary ? 1 : 0, fontSize: 11.5 }, className: 'bae-m-tertiary' }, h(Sym, { name: 'check', size: 11, className: 'bae-success' }), 'In library'),
      h('span', { className: 'bae-m-tertiary', style: { fontSize: 11.5 } }, props.sources || 'Discogs · MusicBrainz'),
      h(Sym, { name: 'right', size: 11, className: props.selected ? 'bae-accent' : 'bae-m-tertiary' }));
  }
  // Several rows selected: the count, then the actions in groups, each with how many it applies to.
  function MacImportBulk(props) {
    var row = function (icon, label, n, strong) { return h('div', { key: label, className: cx('bae-m-bulkrow', !n && 'bae-m-bulkrow-off') }, h(Sym, { name: icon, size: 15, className: strong ? 'bae-accent' : 'bae-muted' }), h('span', { className: strong ? 'bae-strong' : 'bae-muted', style: { flex: 1 } }, label), h(StatusChip, { tone: strong ? 'accent' : 'neutral' }, String(n || 0))); };
    var grp = function (eyebrow, rows) { return h('div', { className: 'bae-m-bulkgrp' }, eyebrow ? h('div', { className: 'bae-m-eyebrowtxt', style: { padding: '8px 12px 2px' } }, eyebrow) : null, rows); };
    var n = Number(props.count || 5);
    return h('div', { className: 'bae-m-bulk' }, h('div', { className: 'bae-m-card', style: { width: 440, padding: 24, display: 'flex', flexDirection: 'column', gap: 12 } },
      h('div', { style: { font: '700 22px var(--font-sans)' } }, n + ' selected'), h('div', { className: 'bae-muted' }, 'Each action applies only to eligible selected folders.'),
      h('div', { className: 'bae-m-bulklist' },
        grp('Import', [row('saveAs', 'Import', props.ready || 3, true), row('xcircle', 'Cancel import of selected', 0), row('grid', 'Combine as One Release', 0, true), row('grid', 'Keep as Separate Releases', 0)]),
        grp('Metadata', [row('refresh', 'Identify selected', n), row('xcircle', 'Stop identifying selected', 0), row('refresh', 'Retry failed identification', 1), row('edit', 'Reset to file metadata', n), row('trash', 'Clear metadata', n)]),
        grp(null, [row('xcircle', 'Skip selected', n), row('left', 'Restore to Pending', 0)]),
        grp(null, [row('folder', 'Reveal in Finder', n)]))),
      props.running ? h('div', { style: { width: 440, display: 'flex', gap: 12, alignItems: 'flex-end' } }, h('div', { style: { flex: 1 } }, h(MacProgress, { label: props.running, value: 0.4 })), h(MacButton, null, 'Cancel')) : null);
  }

  // ==== part: album ====
  // ---- Album actions (macOS) ----
  // The modal host: a scrim over the window, the content clipped to the card radius.
  function MacModal(props) {
    return h('div', { className: 'bae-m-scrim', style: { alignItems: 'center' } }, h('div', { className: 'bae-m-modal', style: { width: Number(props.width || 760), height: props.height ? Number(props.height) : null } }, props.children));
  }
  // A modal's header: title, a second line, its buttons on the right.
  function MacModalHeader(props) {
    return h('div', { className: 'bae-m-modalhead' }, h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: props.large ? 'bae-m-modalhead-large' : 'bae-strong', style: props.large ? null : { fontSize: 15 } }, props.title), props.subtitle ? h('div', { className: 'bae-muted bae-ellipsis', style: { fontSize: 11.5 } }, props.subtitle) : null), props.children);
  }
  // The cover gallery: Source, sections of tiles, the preview with its count, Use This Cover.
  function MacCoverGallery(props) {
    var tiles = function (names, sel) { return h('div', { className: 'bae-m-cgrid' }, list(names).map(function (n, i) { return h('div', { key: i, className: 'bae-m-ctile' }, h('div', { className: cx('bae-m-ctile-img', n === sel && 'bae-m-ctile-sel') }, h(Artwork, { title: n, size: 118, radius: 6 })), h('div', { className: 'bae-ellipsis', style: { fontWeight: 500 } }, n), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.source || 'Release Files')); })); };
    var sec = function (icon, title, n, names) { return h('div', { style: { display: 'flex', flexDirection: 'column', gap: 10 } }, h('div', { className: 'bae-strong', style: { fontSize: 15, display: 'flex', gap: 6, alignItems: 'center' } }, h(Sym, { name: icon, size: 15 }), title, h('span', { className: 'bae-muted', style: { fontWeight: 400 } }, n)), tiles(names, props.selected)); };
    return h('div', { className: 'bae-m-cgal' },
      h(MacModalHeader, { title: props.title || 'Change Cover', large: true }, h(MacButton, null, 'Refresh'), h(MacButton, null, 'Done')),
      h('div', { className: 'bae-m-cgal-body' },
        h('div', { className: 'bae-m-cgal-left' }, h('div', { style: { display: 'flex', gap: 8, alignItems: 'center' } }, 'Source', h(MacPopUpButton, { value: 'All' })),
          sec('photo', 'Current Cover', '1', props.current || 'Low Tide Hours'),
          props.remote === 'unlinked' ? h('div', null, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, 'No linked release'), h('div', { className: 'bae-muted' }, 'Link a Discogs or MusicBrainz release to browse its artwork.'), h(MacButton, null, 'Find release…')) : sec('globe', 'Remote Sources', '2', props.remoteNames || 'Low Tide Hours (Discogs),Low Tide Hours back (Discogs)'),
          sec('folder', 'Release Files', '3', props.files || 'cover.jpg,back.jpg,disc.jpg')),
        h('div', { className: 'bae-m-cgal-right' }, h(Artwork, { title: props.selected || 'Low Tide Hours', size: 300, radius: 6 }), h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.selected || 'Low Tide Hours'), h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.source || 'Release Files'), h('div', { style: { display: 'flex', gap: 10, alignItems: 'center' } }, h(Sym, { name: 'left', size: 12 }), '1 of 6', h(Sym, { name: 'right', size: 12 })))),
      h('div', { className: 'bae-m-cgal-foot' }, h(MacButton, { kind: 'primary' }, 'Use This Cover')));
  }
  // The lightbox: the image on the backdrop, close, arrows, caption, the thumbnail strip.
  function MacLightbox(props) {
    return h('div', { className: 'bae-m-lightbox' },
      h('div', { className: 'bae-m-lb-top' }, props.browse ? h('span', { className: 'bae-m-lb-pill' }, h(Sym, { name: 'grid', size: 12 }), 'Browse all images') : h('span'), h('span', { className: 'bae-m-lb-close' }, h(Sym, { name: 'close', size: 15 }))),
      h('div', { className: 'bae-m-lb-stage' }, h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'left', size: 22 })), props.state === 'failed' ? h('div', { className: 'bae-m-lb-msg' }, h(Sym, { name: 'warning', size: 40 }), 'Couldn’t load image', h(MacButton, null, 'Try again')) : props.state === 'loading' ? h('div', { className: 'bae-m-lb-msg' }, h(MacSpinner, { size: 28 }), 'Loading the image…') : h(Artwork, { title: props.title || 'Low Tide Hours', size: 420, radius: 0 }), h('span', { className: 'bae-m-lb-arrow' }, h(Sym, { name: 'right', size: 22 }))),
      h('div', { className: 'bae-m-lb-cap' }, h('div', null, props.label || 'cover.jpg'), h('div', { style: { opacity: 0.7, fontSize: 11.5 } }, props.source || 'Release Files')),
      h('div', { className: 'bae-m-lb-strip' }, ['Low Tide Hours', 'Low Tide Hours back', 'Low Tide Hours disc', 'Low Tide Hours booklet'].map(function (t, i) { return h('span', { key: i, className: cx('bae-m-lb-thumb', i === 0 && 'bae-m-lb-thumb-on') }, h(Artwork, { title: t, size: 56, radius: 6 })); })));
  }
  // The release's Storage sheet: Storage and Done, the status band, the Files table.
  function MacStorageBand(props) {
    var s = props.state || 'cloud';
    return h('div', { className: 'bae-m-sband' },
      s === 'uploading' ? [h('div', { key: 'c' }, h(StatusChip, { tone: 'activity' }, h(Sym, { name: 'up', size: 10 }), 'Uploading')), h('div', { key: 'b', style: { display: 'flex', gap: 8, alignItems: 'center' } }, h('div', { className: 'bae-m-track', style: { flex: 1 } }, h('div', { className: 'bae-accent-bg', style: { width: '40%' } })), h('span', { className: 'bae-muted bae-num' }, '96 MB of 221.2 MB'))]
        : h('div', { className: 'bae-muted', style: { display: 'flex', gap: 6, alignItems: 'center' } }, h(Sym, { name: { local: 'folder', pinned: 'pin', cloud: 'cloud' }[s], size: 14 }), { local: 'Local', pinned: 'Pinned', cloud: 'Cloud' }[s]),
      props.why ? h('div', { className: 'bae-muted', style: { fontSize: 11.5 } }, props.why) : null,
      s === 'uploading' ? null : h('div', { style: { display: 'flex', gap: 8, flexWrap: 'wrap' } }, list(props.actions || 'Pin,Make Local…,Export…,Save As…').map(function (a) { return h(MacButton, { key: a }, a); })));
  }

  // ==== part: explore ====
  // ---- Explorations: parts for screens bae doesn't have yet ----
  // 1–5 stars, filled up to value; empty draws all five faint. who adds a line of who else rated it.
  function Stars(props) {
    var v = Number(props.value || 0), s = Number(props.size || 14);
    var star = 'M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3 5.8 20.9l1.6-7L2 9.2l7.1-.6z';
    return h('span', { className: 'bae-stars', role: 'img', 'aria-label': v ? v + ' stars' : 'Not rated' },
      [1, 2, 3, 4, 5].map(function (i) { return h('svg', { key: i, width: s, height: s, viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: star, fill: i <= v ? 'var(--accent)' : 'none', stroke: i <= v ? 'var(--accent)' : 'var(--hairline-strong)', strokeWidth: 1.5, strokeLinejoin: 'round' })); }),
      props.who ? h('span', { className: 'bae-muted', style: { marginLeft: 6, fontSize: '0.85em' } }, props.who) : null);
  }
  // A person's picture: initials on a tint when there's no image.
  function Avatar(props) {
    var s = Number(props.size || 32), name = String(props.name || '?');
    var ini = name.split(/\s+/).map(function (p) { return p[0]; }).join('').slice(0, 2).toUpperCase();
    return props.image ? h(Artwork, { title: props.image, size: s, round: true }) : h('span', { className: 'bae-avatar', style: { width: s, height: s, fontSize: s * 0.4 } }, ini);
  }
  // A row of a history or playlist list: when or position, cover, title, artist · album, who, length.
  function MacListRow(props) {
    return h('div', { className: cx('bae-m-lrow', props.selected && 'bae-m-lrow-selected') },
      props.lead ? h('span', { className: 'bae-muted bae-num', style: { width: 56, flex: 'none', fontSize: 11.5 } }, props.lead) : null,
      h(Artwork, { title: props.album || props.title, size: 32, radius: 4 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-ellipsis', style: { fontWeight: 500 } }, props.title), h('div', { className: 'bae-muted bae-ellipsis', style: { fontSize: 11.5 } }, props.sub)),
      props.who ? h('span', { style: { display: 'flex', alignItems: 'center', gap: 6 }, className: 'bae-muted' }, h(Avatar, { name: props.who, size: 18 }), h('span', { style: { fontSize: 11.5 } }, props.who)) : null,
      props.stars ? h(Stars, { value: props.stars, size: 11 }) : null,
      h('span', { className: 'bae-muted bae-num', style: { width: 40, textAlign: 'right', fontSize: 11.5 } }, props.duration));
  }
  function AndroidListRow(props) {
    return h('div', { className: cx('bae-a-lrow', props.selected && 'bae-a-lrow-selected') },
      h(Artwork, { title: props.album || props.title, size: 48, radius: 6 }),
      h('div', { style: { flex: 1, minWidth: 0 } }, h('div', { className: 'bae-a-browse-name' }, props.title), h('div', { className: 'bae-a-browse-count' }, props.sub)),
      props.who ? h(Avatar, { name: props.who, size: 24 }) : null,
      props.trailing ? h('span', { className: 'bae-a-fine bae-muted' }, props.trailing) : null);
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
    MacDetailRow: MacDetailRow, MacSectionHeader: MacSectionHeader, MacWorkDetail: MacWorkDetail, MacHairline: MacHairline, MacContentUnavailable: MacContentUnavailable,
    Sym: Sym, MacStorageWindow: MacStorageWindow, MacTable: MacTable, MacTableRow: MacTableRow, MacReleaseCell: MacReleaseCell, MacStorageChip: MacStorageChip, MacStorageFooter: MacStorageFooter, MacTransferSummary: MacTransferSummary, MacTransferReadout: MacTransferReadout, MacFileRow: MacFileRow, MacFileDetails: MacFileDetails, MacErrorDisclosure: MacErrorDisclosure, MacPopover: MacPopover, MacSyncActivity: MacSyncActivity, AndroidErrorDisclosure: AndroidErrorDisclosure, AndroidSyncActivity: AndroidSyncActivity, AndroidSegmentedButtons: AndroidSegmentedButtons, AndroidDownloadsBar: AndroidDownloadsBar, AndroidDownloadRow: AndroidDownloadRow,
    AndroidSnackbar: AndroidSnackbar, AndroidErrorBanner: AndroidErrorBanner, MacWindowNotice: MacWindowNotice, MacLoadFailure: MacLoadFailure,
    MacImportSidebar: MacImportSidebar, MacImportGroupHeader: MacImportGroupHeader, MacImportRow: MacImportRow, MacImportFolderLine: MacImportFolderLine, MacImportDraft: MacImportDraft, MacImportTable: MacImportTable, MacImportTrackRow: MacImportTrackRow, MacImportSideHeader: MacImportSideHeader, MacImportSheetCaption: MacImportSheetCaption, MacFindOnline: MacFindOnline, MacIdentifierBand: MacIdentifierBand, MacReleaseCard: MacReleaseCard, MacPressingRow: MacPressingRow, MacImportBulk: MacImportBulk,
    MacModal: MacModal, MacModalHeader: MacModalHeader, MacCoverGallery: MacCoverGallery, MacLightbox: MacLightbox, MacStorageBand: MacStorageBand,
    Stars: Stars, Avatar: Avatar, MacListRow: MacListRow, AndroidListRow: AndroidListRow
  });
})();
