  // ---- macOS ----

  var PANES = ['Appearance', 'Library', 'Playback', 'Transfers', 'Storage', 'Import', 'Save as', 'Casting', 'Subsonic', 'Automation', 'About'];

  function MacWelcomeWindow(props) {
    return h('div', { className: 'bae-m-window bae-m-window-plain', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      props.children);
  }

  // A window that walks through one step of joining or adding a device, centred in the window: a headline, an optional line under it, then the step's own content and its buttons (MacStepActions), a section apart. The buttons follow what they act on rather than sitting at the window's foot. The welcome window has no title bar text, so the headline is the only place that says where you are.
  function MacStep(props) {
    return h('div', { className: 'bae-m-step' },
      h('div', { className: 'bae-m-step-head' },
        h('h2', { className: 'bae-m-step-title' }, props.title),
        props.text ? h('p', { className: 'bae-m-step-text' }, props.text) : null),
      props.children);
  }
  // A step's buttons, side by side and centred, the default on the right (list it last).
  function MacStepActions(props) {
    return h('div', { className: 'bae-m-step-actions' }, props.children);
  }

  // A macOS alert: title and message left-aligned, as macOS 26 draws them, then two buttons side by side with the default on the right (list it first); three or more stack. `details` (an error's causes, one a line) adds the inset panel with Copy under the message.
  function MacAlert(props) {
    return h('div', { className: 'bae-m-scrim' },
      h('div', { role: 'alertdialog', 'aria-label': props.title, className: 'bae-m-alert' },
        h('div', { className: 'bae-m-alert-title' }, props.title),
        // `warning`: what the choice costs, as a quiet notice between the title and the message ("Unsent changes will be lost").
        props.warning ? h(MacNotice, { quiet: true, flush: true, tone: 'warning', title: props.warning }) : null,
        props.message ? h('div', { className: 'bae-m-alert-message' }, props.message) : null,
        props.details ? h(MacErrorDetails, { details: props.details }) : null,
        // `suppress`: the checkbox that stops the alert asking again ("Don't show this again"), under the message, as NSAlert's suppression button sits.
        props.suppress ? h(MacCheckbox, { label: props.suppress, on: props.suppressOn }) : null,
        h('div', { className: 'bae-m-alert-buttons' }, props.children)));
  }

  function MacSettingsWindow(props) {
    return h('div', { className: 'bae-m-window', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('nav', { className: 'bae-m-sidebar', 'aria-label': 'Settings panes' },
        h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
        PANES.map(function (p) {
          var flagged = props.status && p === props.statusPane;
          // The open pane is marked current, as the app's sidebar marks its row selected.
          return h('button', { key: p, type: 'button', className: cx('bae-m-pane', p === props.pane && 'bae-m-pane-on'), 'aria-current': p === props.pane ? 'page' : null },
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
    return h('div', { className: cx('bae-m-box', props.elevated && 'bae-m-box-elevated', props.className) }, out);
  }

  // What belongs to the row above it and is more than one setting: a field with its explanation under a switch, say. It joins that row as a nested row does, with no line between them, its text in secondary on the rows' left edge.
  function MacNested(props) {
    return h('div', { className: 'bae-m-nested' }, kids(props.children));
  }

  // An ⓘ after a row's name, for what the name can't say in a word: what Discogs is, say. Clicking it opens a popover with `text` (a board draws the open popover with MacPopover, since a box crops what spills out of it).
  function MacInfoTip(props) {
    return h('button', { type: 'button', className: 'bae-m-infotip', 'aria-label': props.label || 'More info' }, h(StatusGlyph, { tone: 'info', set: 'mac', muted: true, size: 13 }));
  }

  // A port number: a field five digits wide that takes digits only, its label hidden (the row says Port). `state`: normal, focused, error, off. What's wrong with it goes on the row's second line, not under the field.
  function MacPortField(props) {
    return h('div', { className: 'bae-m-portfield' }, h(MacTextField, { label: props.label || 'Port', labelHidden: true, value: props.value, state: props.state }));
  }

  // Text someone may want to paste elsewhere (an address, an id): selectable, with a copy button after it that shows while the pointer is over the text or the button has focus. `after`: words that follow the text but aren't copied (“(Wi-Fi)”), before the button. `hover` draws that state.
  function MacCopyText(props) {
    return h('span', { className: cx('bae-m-copytext', props.hover && 'bae-m-copytext-hover') },
      h('span', { className: 'bae-m-copytext-text' }, props.children != null ? props.children : props.text),
      props.after ? h('span', null, props.after) : null,
      h('button', { type: 'button', className: 'bae-m-copytext-btn', 'aria-label': props.label || 'Copy' }, h(CopyGlyph)));
  }

  function MacRow(props) {
    var trail = kids(props.children);
    // `copy`: one text, after the note; or a list of {text, after}: with more than one, the note ends in a colon and each is a bullet under it.
    var copies = props.copy == null || props.copy === '' ? [] : Array.isArray(props.copy) ? props.copy : [{ text: props.copy }];
    // A row the person clicks is a button, named by its words: on the whole row, or, when the row ends in controls of its own, on its label alone so a button never holds one.
    var tap = !!(props.onClick || props.chevron || props.action || props.destructive || props.button);
    var press = tap ? { role: 'button', 'aria-disabled': props.off ? true : null } : {};
    return h('div', { className: cx('bae-m-row', props.nested && 'bae-m-row-nested', props.nested && props.off && 'bae-m-row-nested-off', !props.nested && props.off && 'bae-off', props.selected && 'bae-m-row-selected', props.action && 'bae-m-row-action', props.destructive && 'bae-m-row-destructive'), role: trail.length ? null : press.role, 'aria-disabled': trail.length ? null : press['aria-disabled'], onClick: props.onClick },
      props.label == null ? null : h('span', Object.assign({ className: 'bae-m-row-label' }, trail.length ? press : null),
        h('span', { className: 'bae-m-row-title' }, h('span', { className: props.strong ? 'bae-m-row-strong' : null }, props.label), props.tag ? h(StatusChip, null, props.tag) : null, props.info ? h(MacInfoTip, { label: 'About ' + props.label }) : null),
        props.note || copies.length ? h('span', { className: cx('bae-m-row-note', props.noteTone && 'bae-tone-' + props.noteTone) },
          copies.length > 1 ? [props.note ? props.note + ':' : null, h('ul', { key: 'l', className: 'bae-m-row-notelist' }, copies.map(function (c, i) { return h('li', { key: i }, h(MacCopyText, { hover: props.copyHover, after: c.after }, c.text)); }))]
            : [props.note, copies.length && props.note ? ' ' : null, copies.length ? h(MacCopyText, { key: 'c', hover: props.copyHover, after: copies[0].after }, copies[0].text) : null]) : null),
      props.value != null && props.value !== '' ? h('span', { className: cx('bae-m-row-value', props.tone && 'bae-tone-' + props.tone) }, props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.value) : null,
      trail.length ? h('span', { className: 'bae-m-row-trail' }, trail) : null,
      props.chevron ? h(Chevron, { className: 'bae-m-row-chevron' }) : null);
  }

  // The row piece every Mac row with a leading icon is built from: `lead` in the leading column (size-lead wide, centred on the first line, kept even when empty so the text edge holds), the content, then `trail`. It has no inset of its own: the row it sits in places it.
  function MacItem(props) {
    var trail = kids(props.trail);
    return h('div', { className: 'bae-m-item' }, h('span', { className: 'bae-m-item-lead' }, props.lead), h('div', { className: 'bae-m-item-body' }, props.children), trail.length ? h('span', { className: 'bae-m-item-trail' }, trail) : null);
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
      // `menu`: the button opens a menu (SwiftUI's `Menu` styled as a button), which macOS reads as a pop-up button.
      'aria-haspopup': props.menu ? 'menu' : null,
      onClick: props.onClick,
      className: cx('bae-m-btn', 'bae-m-btn-' + kind, props.size && props.size !== 'regular' && 'bae-m-btn-' + props.size, props.destructive && 'bae-m-btn-destructive', props.muted && 'bae-m-btn-muted', props.tinted && 'bae-m-btn-tinted')
    };
    if (kind === 'icon') {
      var icon = props.icon || 'copy';
      rest['aria-label'] = props.label || icon;
      return h('button', rest, icon === 'check' ? h(CheckGlyph) : h(CopyGlyph));
    }
    return h('button', rest, props.children != null ? props.children : props.label);
  }

  // A code as people read it: groups of four characters with a space between, the way Settings shows a restore code. Spaces typed or pasted drop first.
  function groupCode(value) {
    var run = String(value || '').replace(/\s+/g, '');
    return run ? run.match(/.{1,4}/g).join(' ') : '';
  }

  // `code`: a field for a restore code or an invite, typed or pasted: several lines tall, monospaced, the code in groups of four as Settings shows it.
  function MacTextField(props) {
    var state = props.state || 'normal';
    var hidden = !!props.labelHidden;
    var bare = hidden || props.label == null;
    // `labelWidth: 'auto'`: a lone field sizes its label to its words, in any language, rather than to a column shared with other fields.
    var auto = props.labelWidth === 'auto';
    var lw = auto ? null : props.labelWidth != null ? Number(props.labelWidth) : 90;
    var isError = state === 'error';
    var line = isError ? props.error : props.note;
    // A code field is a textbox drawn by hand: its placeholder is drawing only, named to readers as aria-placeholder, so the field's value is what's typed.
    var codeHint = props.placeholder || (hidden ? props.label : null);
    var input = props.code ? h('div', { role: 'textbox', 'aria-multiline': true, 'aria-label': props.label, 'aria-placeholder': codeHint || null,
      className: cx('bae-m-input', 'bae-m-codetext', state === 'focused' && 'bae-m-input-focused', isError && 'bae-m-input-error') },
      props.value ? groupCode(props.value) : h('span', { className: 'bae-codetext-placeholder', 'aria-hidden': true }, codeHint)) : h('input', {
      className: cx('bae-m-input', state === 'focused' && 'bae-m-input-focused', isError && 'bae-m-input-error'),
      type: props.secret ? 'password' : 'text',
      defaultValue: props.value || '',
      // A hidden label still names the field to VoiceOver; the placeholder, when given, is what shows.
      placeholder: props.placeholder || (hidden ? props.label : null),
      'aria-label': props.label,
      disabled: state === 'off',
      spellCheck: false
    });
    return h('div', { className: cx('bae-m-field', auto && !bare && 'bae-m-field-auto', state === 'off' && 'bae-off') },
      h('div', { className: 'bae-m-field-line' },
        bare ? null : h('span', { className: 'bae-m-field-label', style: auto ? null : { width: lw } }, props.label),
        input,
        props.children),
      line ? h('div', { className: cx('bae-m-field-note', isError && 'bae-m-field-note-error'), style: auto ? null : { marginLeft: bare ? 0 : lw + 12 } }, line) : null);
  }

  function MacRadio(props) {
    return h('div', { className: 'bae-m-row bae-m-radio', role: 'radio', 'aria-checked': !!props.checked, onClick: props.onClick },
      h('span', { className: cx('bae-m-radio-dot', props.checked && 'bae-m-radio-dot-on'), 'aria-hidden': true }),
      h('span', { className: 'bae-m-row-label' },
        props.label,
        props.note ? h('span', { className: 'bae-m-row-note' }, props.note) : null));
  }

  // macOS's pop-up (a Picker in the menu style): the chosen value as text, then a round button holding the up-and-down chevron.
  // `open` draws its menu as macOS opens it: over the control, the chosen item on the button's line with its words over the value's,
  // `items` the choices in order ("-" for a line between groups), `chosen` the checked one when its words differ from the value, `highlighted` the one under the pointer.
  // A board whose window would crop the menu moves it as macOS keeps a menu on screen: `menuInside` sets its right edge at the control's, `menuTop` its top against the control's.
  function MacPopUpButton(props) {
    var value = props.children != null ? props.children : props.value;
    var btn = h(MacPopUpFace, props, value);
    if (!props.open) return btn;
    var checked = props.chosen || String(value), items = list(props.items), chosen = items.indexOf(checked), seps = items.slice(0, Math.max(chosen, 0)).filter(function (t) { return t === '-'; }).length;
    var top = -(6 + (Math.max(chosen, 0) - seps) * 24 + seps * 11);
    return h('span', { className: 'bae-m-popupbutton-open' }, btn,
      h('div', { className: 'bae-m-popupbutton-menu', style: props.menuInside ? { top: props.menuTop != null ? Number(props.menuTop) : top, left: 'auto', right: -6 } : { top: props.menuTop != null ? Number(props.menuTop) : top } }, h(MacMenu, null, items.map(function (t, i) {
        return t === '-' ? h(MacMenuItem, { key: i, divider: true }) : h(MacMenuItem, { key: i, checked: t === checked, highlighted: t === (props.highlighted || checked) }, t);
      }))));
  }
  function MacPopUpFace(props) {
    return h('button', { type: 'button', className: 'bae-m-popupbutton', 'aria-haspopup': 'menu', disabled: !!props.off, style: props.width ? { width: Number(props.width), justifyContent: 'space-between' } : null },
      h('span', { className: 'bae-m-popupbutton-value' }, props.children),
      h('span', { className: 'bae-m-popupbutton-mark', 'aria-hidden': true }, h('svg', { width: 8, height: 12, viewBox: '0 0 8 12', fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M1.5 4.5 4 2l2.5 2.5M1.5 7.5 4 10l2.5-2.5' }))));
  }


  function MacSegmentedControl(props) {
    var options = String(props.options || '').split(',').map(function (o) { return o.trim(); }).filter(Boolean);
    return h('span', { className: 'bae-m-segmented', role: 'radiogroup' },
      options.map(function (o) {
        return h('span', { key: o, role: 'radio', 'aria-checked': o === props.selected, className: cx('bae-m-segment', o === props.selected && 'bae-m-segment-on') }, o);
      }));
  }

  // DisclosureGroup in a grouped form: its own box, the chevron leading inside the label row and turning down when open; the rows it shows are indented to the label.
  function MacDisclosure(props) {
    var s = React.useState(!!props.open);
    var open = s[0];
    var toggle = h('button', { type: 'button', className: 'bae-m-row bae-m-disclosure-toggle', 'aria-expanded': open, onClick: function () { s[1](!open); } },
      h('span', { className: 'bae-m-disclosure-lead' }, h(Chevron, { className: cx('bae-m-disclosure-mark', open && 'bae-m-disclosure-mark-open') })),
      props.label || 'Advanced');
    return h(MacBox, { className: 'bae-m-disclosure' }, toggle, open ? props.children : null);
  }

  function MacSheet(props) {
    var panel = h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-m-sheet', style: { width: Number(props.width || 460) } },
      props.title ? h('div', { className: 'bae-m-sheet-title' }, props.title) : null,
      // `message`: the sheet's line under its title, in secondary text, as an alert's message sits under its title.
      h('div', { className: 'bae-m-sheet-body' }, props.message ? h('div', { className: 'bae-m-sheet-message' }, props.message) : null, props.children));
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

  // A labelled bar: `label` shows above it and names it; `name` names a bar whose words sit elsewhere, without showing them.
  function Progress(prefix) {
    return function (props) {
      // A board passes value as text ("0.45"), so a number in any form is a known amount; no value is a wait of no known length, a moving bar.
      var value = props.value == null || props.value === '' ? NaN : Number(props.value);
      var known = !isNaN(value);
      return h('div', { className: prefix + '-progress' },
        props.label ? h('div', null, props.label) : null,
        h(ProgressBar, { value: known ? value : null, android: prefix === 'bae-a', label: props.label || props.name }));
    };
  }
  // macOS draws its indeterminate spinner as eight spokes fading round the wheel, turning a spoke at a time; Android draws Material's ring with an arc.
  function Spinner(prefix, size) {
    return function (props) {
      var s = Number(props.size || size);
      var spin = prefix === 'bae-m'
        ? h('svg', { className: 'bae-spinner bae-spinner-spokes', width: s, height: s, viewBox: '0 0 24 24', role: 'progressbar', 'aria-label': props.label || 'Loading', fill: 'none' },
          [0, 1, 2, 3, 4, 5, 6, 7].map(function (i) { return h('line', { key: i, x1: 12, y1: 2.5, x2: 12, y2: 7.5, stroke: 'currentColor', strokeWidth: 2.4, strokeLinecap: 'round', strokeOpacity: 1 - i * 0.11, transform: 'rotate(' + (-i * 45) + ' 12 12)' }); }))
        : h('svg', { className: 'bae-spinner', width: s, height: s, viewBox: '0 0 24 24', role: 'progressbar', 'aria-label': props.label || 'Loading', fill: 'none' },
          h('circle', { cx: 12, cy: 12, r: 10.5, stroke: 'currentColor', strokeOpacity: 0.2, strokeWidth: 2.5 }),
          h('path', { d: 'M12 1.5a10.5 10.5 0 0 1 10.5 10.5', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' }));
      return props.label ? h('span', { className: prefix + '-spinner' }, spin, h('span', null, props.label)) : h('span', { className: prefix + '-spinner' }, spin);
    };
  }
  var MacSpinner = Spinner('bae-m', 16);
  var AndroidSpinner = Spinner('bae-a', 24);
  var MacProgress = Progress('bae-m');
  var AndroidProgress = Progress('bae-a');

  function MacNote(props) {
    return h('div', { className: 'bae-m-note' }, props.children);
  }

  // How Save as… names each track's file, one pattern for every format: the pattern's values as macOS's own tokens, with the text typed between them, a preview of a name it makes right under the field, as its help line, and the values it can add. `pattern`: the text, with each value in brackets ("[Artist] - [Album]"); `add`: comma-separated; `preview`: the name it shows; `error`: why the typed text makes no name, in the preview's place.
  function MacFileNameField(props) {
    var split = function (s) { return String(s || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean); };
    var pieces = String(props.pattern || '').split(/(\[[^\]]*\])/).filter(function (x) { return x !== ''; });
    // `label` sits above the field, at the row's leading edge where a MacRow's title sits, so the field runs the box's full width.
    return h('div', { className: 'bae-m-filename' },
      props.label == null ? null : h('span', { className: 'bae-m-filename-label' }, props.label),
      h('div', { className: cx('bae-m-tokenfield', props.error && 'bae-m-tokenfield-error'), 'aria-label': props.label, 'aria-invalid': props.error ? true : null }, pieces.map(function (t, i) { return t.charAt(0) === '[' ? h('span', { key: i, className: 'bae-m-token' }, t.slice(1, -1)) : h('span', { key: i, className: 'bae-m-tokentext' }, t); })),
      props.error ? h('div', { className: 'bae-m-field-note bae-m-field-note-error' }, props.error)
        : props.preview ? h('div', { className: 'bae-m-filename-preview' }, 'Preview: ', h('span', { className: 'bae-mono' }, props.preview)) : null,
      h('div', { className: 'bae-m-filename-add' }, 'Add:', split(props.add).map(function (t) { return h('span', { key: t, className: 'bae-m-tokenadd' }, t); })));
  }

  function MacSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-m-switch', on && 'bae-m-switch-on', props.off && 'bae-m-switch-off'), disabled: !!props.off, onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-m-switch-knob' }));
  }

  // The accents in the order the appearance settings offer them.
  var ACCENTS = ['blue', 'orchid', 'pink', 'coral', 'gold', 'lime', 'mint', 'sky'];

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

  // `divider`: a line between groups of items, as NSMenu's separator item. `off`: an item that can't be chosen now, greyed, as NSMenu disables one.
  function MacMenuItem(props) {
    if (props.divider) return h('div', { className: 'bae-m-menu-sep', role: 'separator' });
    return h('div', { className: cx('bae-m-menu-item', props.highlighted && !props.off && 'bae-m-menu-item-on', props.off && 'bae-m-menu-item-off'), role: 'menuitemradio', 'aria-checked': !!props.checked, 'aria-disabled': props.off ? true : null },
      h('span', { className: 'bae-m-menu-check', 'aria-hidden': true }, props.checked ? h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.6, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M5 12.5l4.5 4.5L19 7.5' })) : null),
      props.children);
  }
