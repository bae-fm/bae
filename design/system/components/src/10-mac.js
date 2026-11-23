  // ---- macOS ----

  var PANES = ['Appearance', 'Library', 'Playback', 'Transfers', 'Storage', 'Import', 'Save as', 'Subsonic', 'Automation', 'About'];

  // The traffic lights at a window's top left: close, minimize, zoom.
  // `inset`: over an inset sidebar, which sits 8pt in from the window's edge, so the lights keep their place on the sidebar's corner.
  function macLights(inset) { return h('div', { className: cx('bae-m-lights', inset && 'bae-m-lights-inset'), 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)); }

  // The welcome window: the rounded window, 690 by 600 unless given, its traffic lights at the top left, its screens stacked inside.
  function MacWelcomeWindow(props) {
    return h('div', { className: 'bae-m-window bae-m-window-column', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      macLights(), props.children);
  }

  // The camera's picture in the Scan a code sheet, 300pt tall with a card's corners. `state`: `starting` (the default) while the camera opens, `denied` when camera access is off, `unavailable` with no camera, `camera` once its picture shows. Until the picture shows, the area says why in secondary text on the well.
  function MacQRScanner(props) {
    var state = props.state || 'starting';
    var words = { starting: 'Starting camera…', denied: 'Camera access is off. Enable it in System Settings, or paste the code below.', unavailable: 'No camera available. Paste the code below instead.' }[state];
    return h('div', { className: cx('bae-m-qrscan', state === 'camera' && 'bae-m-qrscan-camera'), role: state === 'camera' ? 'img' : null, 'aria-label': state === 'camera' ? 'Camera' : null }, words ? h('span', null, words) : null);
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

  // A macOS alert: title and message left-aligned, as macOS 26 draws them, then two buttons side by side with the default on the right (list it first); three or more stack, and so do two when a label is wider than its half of the row. `details` (an error's causes, one a line) adds the inset panel with Copy under the message.
  function MacAlert(props) {
    var row = React.useRef(null), stackState = React.useState(false);
    // Measured once laid out: a side-by-side button whose words overflow it stacks the pair.
    React.useLayoutEffect(function () {
      var el = row.current;
      if (!el || stackState[0]) return;
      var over = Array.prototype.some.call(el.querySelectorAll('button'), function (b) { return b.scrollWidth > b.clientWidth + 0.5; });
      if (over) stackState[1](true);
    });
    return h('div', { className: 'bae-m-scrim' },
      h('div', { role: 'alertdialog', 'aria-label': props.title, className: 'bae-m-face-sheet bae-m-alert' },
        h('div', { className: 'bae-m-alert-title' }, props.title),
        // `warning`: what the choice costs, as a quiet notice between the title and the message ("Unsent changes will be lost").
        props.warning ? h(MacNotice, { quiet: true, tone: 'warning', title: props.warning }) : null,
        props.message ? h('div', { className: 'bae-m-alert-message' }, props.message) : null,
        props.details ? h(MacErrorDetails, { details: props.details }) : null,
        // `suppress`: the checkbox that stops the alert asking again ("Don't show this again"), under the message, as NSAlert's suppression button sits.
        props.suppress ? h(MacCheckbox, { label: props.suppress, on: props.suppressOn }) : null,
        h('div', { ref: row, className: cx('bae-m-alert-buttons', stackState[0] && 'bae-m-alert-buttons-stacked') }, props.children)));
  }

  // The round back button before a page's heading, named for where it goes back to (`to`): a Settings page under another, a playlist open in the side pane.
  function MacBackButton(props) {
    return h('button', { type: 'button', className: 'bae-m-back', 'aria-label': 'Back to ' + props.to },
      h('svg', { width: 10, height: 10, viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: 'M15.41 16.59L10.83 12l4.58-4.59L14 6l-6 6 6 6 1.41-1.41z' })));
  }

  // The Settings window's list of panes, on its inset panel; the window's traffic lights sit over its top. `pane` is the open pane, marked current as the app's sidebar marks its row selected; `status` puts a chip in `statusTone` (warning unless given) on `statusPane`.
  function MacSettingsSidebar(props) {
    return h('nav', { className: 'bae-m-sidebar', 'aria-label': 'Settings panes' },
      PANES.map(function (p) {
        var flagged = props.status && p === props.statusPane;
        return h('button', { key: p, type: 'button', className: cx('bae-m-pane', p === props.pane && 'bae-m-pane-on'), 'aria-current': p === props.pane ? 'page' : null, 'aria-label': flagged ? p + ', ' + props.status : null },
          h('span', null, p),
          flagged ? h(StatusChip, { tone: props.statusTone || 'warning' }, props.status) : null);
      }));
  }

  // A Settings pane's heading, with the back button before it on a page inside a pane (`back`, where the button goes back to).
  function MacPageHeading(props) {
    return h('div', { className: 'bae-m-heading' },
      props.back ? h(MacBackButton, { to: props.back }) : null,
      h('h1', null, props.title));
  }

  // The Settings window, 690 by 600 unless given: its traffic lights over the panes' sidebar, beside the open pane under its heading (`title`, with `back` on a page inside a pane). `front`: the window stands in front of another window, so it casts the shadow macOS draws under a front window.
  function MacSettingsWindow(props) {
    return h('div', { className: cx('bae-m-window', props.front && 'bae-m-window-front'), style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h(MacSettingsSidebar, { pane: props.pane, status: props.status, statusPane: props.statusPane, statusTone: props.statusTone }),
      macLights(true),
      h('main', { className: 'bae-m-main' },
        h(MacPageHeading, { title: props.title, back: props.back }),
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
    return h('div', { className: cx('bae-m-box', props.className) }, out);
  }

  // What belongs to the row above it and is more than one setting: a field with its explanation under a switch, say. It joins that row as a nested row does, with no line between them, its text in secondary on the rows' left edge.
  function MacNested(props) {
    return h('div', { className: 'bae-m-nested' }, kids(props.children));
  }

  // An ⓘ after a row's name, for what the name can't say in a word: what Discogs is, say. Clicking it opens a popover with `text` (a board draws the open popover with MacPopover, since a box crops what spills out of it).
  function MacInfoTip(props) {
    return h('button', { type: 'button', className: 'bae-m-infotip', 'aria-label': props.label || 'More info' }, h(StatusGlyph, { tone: 'info', set: 'mac', muted: true, size: 'medium' }));
  }

  // A port number: a field five digits wide that takes digits only, its label hidden (the row says Port). `state`: normal, focused, error, off. What's wrong with it goes on the row's second line, not under the field.
  function MacPortField(props) {
    return h('div', { className: 'bae-m-portfield' }, h(MacTextField, { label: props.label || 'Port', labelHidden: true, value: props.value, state: props.state }));
  }

  // A code the person reads off a receiver's screen and types: one box per digit (`digits`, which core states). The field rings the box where the next digit goes while the field is `focused`, and shows each digit as the person types it. `state`: normal, focused, error, off. What's wrong goes under the boxes, in red.
  function MacPinField(props) {
    var state = props.state || 'normal';
    var n = props.digits || 4;
    var value = String(props.value || '');
    var isError = state === 'error';
    var cells = [];
    for (var i = 0; i < n; i++) {
      cells.push(h('span', { key: i, className: cx('bae-m-pin-cell', state === 'focused' && i === Math.min(value.length, n - 1) && 'bae-m-pin-cell-focused', isError && 'bae-m-pin-cell-error') }, value.charAt(i)));
    }
    return h('div', { className: cx('bae-m-pin', state === 'off' && 'bae-off') },
      h('div', { role: 'textbox', 'aria-label': props.label || 'PIN', 'aria-disabled': state === 'off' ? true : null, className: 'bae-m-pin-cells' }, cells),
      isError && props.error ? h(MacErrorLine, null, props.error) : null);
  }

  // Text someone may want to paste elsewhere (an address, an id): selectable, with a copy button after it that shows while the pointer is over the text or the button has focus. `after`: words that follow the text but aren't copied (“(Wi-Fi)”), before the button. `hover` draws that state.
  function MacCopyText(props) {
    return h('span', { className: cx('bae-m-copytext', props.hover && 'bae-m-copytext-hover') },
      h('span', { className: 'bae-m-copytext-text' }, props.children != null ? props.children : props.text),
      props.after ? h('span', null, props.after) : null,
      h('button', { type: 'button', className: 'bae-m-copytext-btn', 'aria-label': props.label || 'Copy' }, h(CopyGlyph, { size: 'small' })));
  }

  function MacRow(props) {
    var trail = kids(props.children);
    // `copy`: one text, after the note; or a list of {text, after}: with more than one, the note ends in a colon and each is a bullet under it.
    var copies = props.copy == null || props.copy === '' ? [] : Array.isArray(props.copy) ? props.copy : [{ text: props.copy }];
    // A row the person clicks is a button, named by its words: on the whole row, or, when the row ends in controls of its own, on its label alone so a button never holds one.
    var tap = !!(props.onClick || props.chevron || props.action || props.destructive || props.button);
    // macOS names a clickable row from its texts joined with ", " ("Devices, 2 devices"), where a browser joins them with spaces. A string note is one of them; a note of other content is not.
    var named = function (parts) { return parts.filter(function (x) { return typeof x === 'string' && x !== ''; }).join(', '); };
    // A chosen row (`selected`) is the current one, which the Mac reads as the row's selected state.
    var press = tap ? { role: 'button', 'aria-disabled': props.off ? true : null, 'aria-current': props.selected ? 'true' : null } : {};
    var rowName = tap ? named([props.label, props.tag, props.note, props.value]) : '';
    var labelName = tap ? named([props.label, props.tag, props.note]) : '';
    return h('div', { className: cx('bae-m-row', props.nested && 'bae-m-row-nested', props.nested && props.off && 'bae-m-row-nested-off', !props.nested && props.off && 'bae-off', props.selected && 'bae-m-row-selected', props.action && 'bae-m-row-action', props.destructive && 'bae-m-row-destructive'), role: trail.length ? null : press.role, 'aria-label': tap && !trail.length && rowName ? rowName : null, 'aria-disabled': trail.length ? null : press['aria-disabled'], 'aria-current': trail.length ? null : press['aria-current'], onClick: props.onClick },
      // `avatar`: a person's name, whose picture (`avatarImage`) or initials lead the row.
      props.avatar != null ? h('span', { className: 'bae-m-row-lead' }, h(Avatar, { name: props.avatar, image: props.avatarImage, size: 28 })) : null,
      props.label == null ? null : h('span', Object.assign({ className: 'bae-m-row-label' }, trail.length ? press : null, trail.length && labelName ? { 'aria-label': labelName } : null),
        h('span', { className: 'bae-m-row-title' }, h('span', { className: props.strong ? 'bae-m-row-strong' : null }, props.label), props.tag ? h(StatusChip, null, props.tag) : null, props.info ? h(MacInfoTip, { label: props.infoLabel || 'About ' + props.label }) : null),
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
    // `centerLead`, `centerTrail`: the lead or the trail sits at the middle of the body's lines, not on its first line. `wide`: the content starts 10px (`space-group` less `space-line`) after the lead, not `space-related`.
    return h('div', { className: cx('bae-m-item', props.centerLead && 'bae-m-item-center-lead', props.centerTrail && 'bae-m-item-center-trail', props.wide && 'bae-m-item-wide') }, h('span', { className: 'bae-m-item-lead' }, props.lead), h('div', { className: 'bae-m-item-body' }, props.children), trail.length ? h('span', { className: 'bae-m-item-trail' }, trail) : null);
  }
  // A row of a MacBox whose label takes a column labelWidth wide and whose field fills the rest.
  function MacFieldRow(props) {
    var fieldId = React.useId();
    var children = kids(props.children);
    var first = children.length ? inner(children[0]) : null;
    var namesField = props.label != null && first && first.type === MacTextField && !first.props.code && first.props.label == null;
    if (namesField) children[0] = rewrap(children[0], React.cloneElement(first, { inputId: fieldId }));
    return h('div', { className: cx('bae-m-row', 'bae-m-fieldrow', props.alignText && 'bae-m-fieldrow-aligned') },
      props.label == null ? null : h(namesField ? 'label' : 'span', { htmlFor: namesField ? fieldId : null, className: 'bae-m-fieldrow-label', style: { width: Number(props.labelWidth || 90) } }, props.label),
      h('div', { className: 'bae-m-fieldrow-content' }, children));
  }

  function MacButton(props) {
    var kind = props.kind || 'secondary';
    var rest = {
      type: 'button',
      disabled: !!props.off,
      // `menu`: the button opens a menu (SwiftUI's `Menu` styled as a button), which macOS reads as a pop-up button.
      'aria-haspopup': props.menu ? 'menu' : null,
      onClick: props.onClick,
      className: cx('bae-m-btn', 'bae-m-btn-' + kind, props.size && props.size !== 'regular' && 'bae-m-btn-' + props.size, props.destructive && 'bae-m-btn-destructive', props.muted && 'bae-m-btn-muted', props.neutral && 'bae-m-btn-neutral', props.tinted && 'bae-m-btn-tinted')
    };
    if (kind === 'icon') {
      var icon = props.icon || 'copy';
      rest['aria-label'] = props.label || icon;
      return h('button', rest, icon === 'check' ? h(CheckGlyph, { size: 'medium' }) : h(CopyGlyph, { size: 'medium' }));
    }
    // `label` beside words names a button whose words don't say what it does, such as the ⋯ that opens the Metadata menu.
    if (props.children != null && props.label != null) rest['aria-label'] = props.label;
    return h('button', rest, props.children != null ? props.children : props.label);
  }

  // A text field drawn as the text it holds, with no box of its own, for a field that sits inside another control's frame (a search capsule, a filter row): `label` names it, `placeholder` shows while it is empty.
  function bareInput(props) {
    return h('input', { type: 'text', className: cx('bae-m-bareinput', props.className), defaultValue: props.value || '', placeholder: props.placeholder, 'aria-label': props.label, spellCheck: false });
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
      id: props.inputId, type: props.secret ? 'password' : 'text',
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
      line ? h('div', { className: 'bae-m-field-note', style: auto ? null : { marginLeft: bare ? 0 : lw + 12 } }, isError ? h(MacErrorLine, null, line) : line) : null);
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
  // `label` names the pop-up by what it chooses (Import to, Album), as the app's Picker is named by its title, and the value it shows is its value.
  function MacPopUpFace(props) {
    var named = props.label != null ? { role: 'combobox', 'aria-label': props.label, 'aria-expanded': !!props.open } : { 'aria-haspopup': 'menu' };
    return h('button', Object.assign({ type: 'button', className: 'bae-m-popupbutton', disabled: !!props.off, style: props.width ? { width: Number(props.width), justifyContent: 'space-between' } : null }, named),
      h('span', { className: 'bae-m-popupbutton-value' }, props.children),
      h('span', { className: 'bae-m-popupbutton-mark', 'aria-hidden': true }, h('svg', { width: 8, height: 12, viewBox: '0 0 8 12', fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M1.5 4.5 4 2l2.5 2.5M1.5 7.5 4 10l2.5-2.5' }))));
  }


  function MacSegmentedControl(props) {
    var options = String(props.options || '').split(',').map(function (o) { return o.trim(); }).filter(Boolean);
    // `off` dims the whole control while none of its options can be chosen.
    // `label` names the group, as VoiceOver reads it.
    return h('span', { className: cx('bae-m-segmented', props.off && 'bae-off'), role: 'radiogroup', 'aria-label': props.label || null, 'aria-disabled': props.off ? true : null },
      options.map(function (o) {
        return h('span', { key: o, role: 'radio', 'aria-checked': o === props.selected, className: cx('bae-m-segment', o === props.selected && 'bae-m-segment-on') }, o);
      }));
  }

  // The system stepper: a small rounded box of an up and a down arrow beside a count, named by what it counts (`label`).
  // macOS reads it as one stepper with that label, its arrows part of the control as a scroll bar's buttons are, so the arrows stay out of the reading order. `off`: the stepper and its arrows are disabled.
  function MacStepper(props) {
    var off = !!props.off;
    return h('span', { className: cx('bae-m-stepper', off && 'bae-off'), role: 'spinbutton', 'aria-label': props.label, 'aria-disabled': off || null },
      h('button', { type: 'button', className: 'bae-plainbtn', 'aria-hidden': true, tabIndex: -1, disabled: off }, h(Glyph, { set: 'mac', name: 'down', size: 'small', className: 'bae-m-stepper-up' })),
      h('button', { type: 'button', className: 'bae-plainbtn', 'aria-hidden': true, tabIndex: -1, disabled: off }, h(Glyph, { set: 'mac', name: 'down', size: 'small' })));
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
    var panel = h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-m-face-sheet bae-m-sheet', style: { width: Number(props.width || 460) } },
      props.title ? h('div', { className: 'bae-m-sheet-title' }, props.title) : null,
      // `message`: the sheet's line under its title, in secondary text, as an alert's message sits under its title.
      h('div', { className: 'bae-m-sheet-body' }, props.message ? h('div', { className: 'bae-m-sheet-message' }, props.message) : null, props.children));
    if (!props.overlay) return panel;
    return h('div', { className: 'bae-m-scrim' }, panel);
  }

  // `split` sets the first child apart on the left, or the first `split` children when it is a number, in their own box so a wrapper around them can't swallow the spacing.
  function MacSheetFooter(props) {
    var list = kids(props.children);
    var lead = props.split === true || props.split === 'true' ? 1 : Number(props.split) || 0;
    if (!lead || list.length <= lead) return h('div', { className: 'bae-m-sheet-footer' }, list);
    return h('div', { className: 'bae-m-sheet-footer' },
      h('div', { className: 'bae-m-sheet-footer-lead' }, list.slice(0, lead)), list.slice(lead));
  }

  // A labelled bar: `label` shows above it and names it; `name` names a bar whose words sit elsewhere, without showing them. The Mac's bar is 4pt.
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

  // A labelled control in a sheet's form: the label above the control and its note under it, all on the form's one left edge (`label`, `note`; the control is the child).
  // The control takes the field's label as its name, as VoiceOver reads it, and draws no label of its own: a text field's label is hidden, a segmented control names its group.
  function MacFormField(props) {
    var named = kids(props.children).map(function (c) { var p = inner(c); return p && typeof p.type === 'function' ? rewrap(c, React.cloneElement(p, { label: props.label, labelHidden: true })) : c; });
    return h('div', { className: 'bae-m-formfield' }, h('span', { className: 'bae-m-formfield-label' }, props.label), named, props.note ? h('div', { className: 'bae-m-formfield-note' }, props.note) : null);
  }

  function MacNote(props) {
    return h('div', { className: 'bae-m-note' }, props.children);
  }

  // One value of a file name pattern, as macOS's token field draws it: the system's blue, its words in the field's font, small corners, no ×.
  function MacToken(props) { return h('span', { className: 'bae-m-token' }, props.children); }

  // A red line under a field that says what is wrong with what the person typed, on the field's leading edge.
  function MacErrorLine(props) { return h('div', { className: 'bae-m-errorline' }, props.children); }

  // How Save as… names each track's file, one pattern for every format: the pattern's values as macOS's own tokens, with the text typed between them, a preview of a name it makes right under the field, as its help line, and the values it can add. `pattern`: the text, with each value in brackets ("[Artist] - [Album]"); `add`: comma-separated; `preview`: the name it shows; `error`: why the typed text makes no name, in the preview's place.
  function MacFileNameField(props) {
    var split = function (s) { return String(s || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean); };
    var pieces = String(props.pattern || '').split(/(\[[^\]]*\])/).filter(function (x) { return x !== ''; });
    // `label` sits above the field, at the row's leading edge where a MacRow's title sits, so the field runs the box's full width.
    return h('div', { className: 'bae-m-filename' },
      h('span', { className: 'bae-m-filename-label' }, props.label),
      h('div', { className: cx('bae-m-tokenfield', props.error && 'bae-m-tokenfield-error'), role: 'textbox', 'aria-label': props.label, 'aria-invalid': props.error ? true : null }, pieces.map(function (t, i) { return t.charAt(0) === '[' ? h(MacToken, { key: i }, t.slice(1, -1)) : h('span', { key: i, className: 'bae-m-tokentext' }, t); })),
      props.error ? h(MacErrorLine, null, props.error)
        : props.preview ? h('div', { className: 'bae-m-filename-preview' }, 'Preview: ', h('span', { className: 'bae-mono' }, props.preview)) : null,
      h('div', { className: 'bae-m-filename-add' }, 'Add:', split(props.add).map(function (t) { return h('button', { key: t, type: 'button', className: 'bae-plainbtn bae-m-tokenadd' }, t); })));
  }

  function MacSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-m-switch', on && 'bae-m-switch-on', props.off && 'bae-m-switch-off'), disabled: !!props.off, onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-m-switch-knob' }));
  }

  // The accents in the order the appearance settings offer them.
  var ACCENTS = ['blue', 'orchid', 'pink', 'lime', 'mint', 'sky'];

  // `named`: the group carries its name for a screen reader, as the Mac's does; Android's group is unnamed, as Compose's selectable group is, and the Accent row over it names it.
  function accentPicker(className, named) {
    return function (props) {
      var s = React.useState(props.selected || 'pink');
      var chosen = s[0];
      return h('div', { className: className, role: 'radiogroup', 'aria-label': named ? props.label || 'Accent' : null }, ACCENTS.map(function (a) {
        return h('button', { key: a, type: 'button', role: 'radio', 'aria-checked': a === chosen, 'aria-label': a.charAt(0).toUpperCase() + a.slice(1),
          className: cx('bae-swatch', a === chosen && 'bae-swatch-on'), style: { background: 'var(--accent-' + a + '-fill)' }, onClick: function () { s[1](a); } });
      }));
    };
  }
  var MacAccentPicker = accentPicker('bae-m-accents', true);
  var AndroidAccentPicker = accentPicker('bae-a-accents', false);

  // `more`: the menu runs past the screen's bottom edge, and NSMenu's scroll arrow ends it.
  function MacMenu(props) {
    return h('div', { className: 'bae-m-menu', role: 'menu' }, props.children, props.more ? h('div', { className: 'bae-m-menu-more', 'aria-hidden': true }, '▾') : null);
  }

  // `divider`: a line between groups of items, as NSMenu's separator item. `off`: an item that can't be chosen now, greyed, as NSMenu disables one.
  // One line of a MacMenu: an item (checked, highlighted, off), a `heading` over a group of items, a `divider`, or a `submenu`, whose › opens more items and which may carry an `accessory` before the ›: `problem` (a red ! in a circle, for a folder whose scan failed or that bae is not watching), `scanning` (a spinner) or `network` (a globe), with `help` as its hover text.
  function MacMenuItem(props) {
    if (props.divider) return h('div', { className: 'bae-m-menu-sep', role: 'separator' });
    if (props.heading) return h('div', { className: 'bae-m-menu-heading', role: 'presentation' }, props.children);
    var on = props.highlighted && !props.off;
    var accessory = props.submenu && props.accessory ? h('span', { className: cx('bae-m-menu-accessory', !on && props.accessory === 'problem' && 'bae-m-menu-accessory-problem') },
      props.accessory === 'problem' ? h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2.2, strokeLinecap: 'round' }, h('circle', { cx: 12, cy: 12, r: 9.5 }), h('path', { d: 'M12 7v6M12 16.6v.4' }))
        : props.accessory === 'scanning' ? h(MacSpinner, { size: 11 })
        : h(Sym, { name: 'globe', size: 11 })) : null;
    return h('div', { className: cx('bae-m-menu-item', on && 'bae-m-menu-item-on', props.off && 'bae-m-menu-item-off'), role: props.submenu ? 'menuitem' : 'menuitemradio', 'aria-checked': props.submenu ? null : !!props.checked, 'aria-haspopup': props.submenu ? 'menu' : null, 'aria-disabled': props.off ? true : null, title: props.help || null },
      h('span', { className: 'bae-m-menu-check', 'aria-hidden': true }, props.checked ? h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.6, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M5 12.5l4.5 4.5L19 7.5' })) : null),
      props.children,
      props.submenu ? h('span', { className: 'bae-m-menu-trail' }, accessory,
        h('svg', { width: 9, height: 9, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 3.2, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M9 5l7 7-7 7' }))) : null);
  }
