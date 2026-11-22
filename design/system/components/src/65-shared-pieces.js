  // ---- Pieces that several screens share: welcome, cloud home, restore code, sync status, invites ----

  // The welcome window's first choices when this Mac has no library: create one, or add one from a code another device shows.
  function MacWelcomeActions() {
    return h('div', { className: 'bae-mac bae-m-welcome-actions' },
      h('div', { className: 'bae-m-welcome-actions-stack' },
        h(MacButton, { kind: 'primary', size: 'large' }, 'Create new library…'),
        h('div', { className: 'bae-m-welcome-actions-pair' },
          h('div', { className: 'bae-m-welcome-actions-half' }, h(MacButton, { size: 'large' }, 'Scan code')),
          h('div', { className: 'bae-m-welcome-actions-half' }, h(MacButton, { size: 'large' }, 'Enter code')))));
  }

  function AndroidWelcomeActions() {
    return h('div', { className: 'bae-a-welcome-actions' },
      h('div', { className: 'bae-a-welcome-actions-stack' },
        h(AndroidButton, { large: true }, 'Scan code'),
        h(AndroidButton, { kind: 'outlined', large: true }, 'Enter code')));
  }

  // The libraries on this device, as the welcome window lists them. `state`: '' (both open), 'removing' (bae is leaving the first), 'open-failed' (the first could not open), 'unreadable' or 'damaged-join' (a third row that bae cannot read).
  function MacWelcomeLibraries(props) {
    var s = props.state || '';
    var removing = s === 'removing';
    return h('div', { className: 'bae-mac bae-m-welcome-libraries' },
      h(MacSection, { title: 'Your libraries' },
        h(MacBox, null,
          removing ? null : h(MacRow, { label: 'Home music', strong: true, button: true }, h(MacMoreMenu)),
          s === 'open-failed' ? h(MacNested, null, h(MacNotice, { tone: 'danger', title: 'Could not open', subtitle: 'Something is wrong with this library’s data.', secondary: 'Details', action: 'Reset from cloud…', buttons: 'below' })) : null,
          removing ? h(MacRow, { label: 'Home music', strong: true, note: 'Leaving the library…' }, h('div', { className: 'bae-m-welcome-libraries-wait' }, h(MacSpinner))) : null,
          h(MacRow, { label: 'Odile’s records', strong: true, button: true, off: removing }, h(MacMoreMenu, { off: removing })),
          s === 'damaged-join' ? h(MacRow, { label: 'Unfinished join that cannot be read', strong: true, note: '3b9e07d4-51a2-4c8f-b6e3-0f2d9a7c4e81' }, h(MacButton, null, 'Discard…')) : null,
          s === 'unreadable' ? h(MacRow, { label: 'Library that cannot be read', strong: true, note: '6f1c2a9e-4b7d-4e21-9a0c-3d5e8b2f7c14' }, h(MacButton, null, 'Remove…')) : null),
        h(MacSheetFooter, null, h(MacButton, { off: removing, menu: true }, 'Add library…'))));
  }

  // `state` as MacWelcomeLibraries', without 'open-failed'.
  function AndroidWelcomeLibraries(props) {
    var s = props.state || '';
    var removing = s === 'removing';
    return h('div', { className: 'bae-a-welcome-libraries' },
      h(AndroidSection, { title: 'Your libraries' }),
      h(AndroidCard, null,
        removing ? null : h(AndroidRow, { title: 'Home music', button: true }, h(AndroidMoreMenu)),
        removing ? h(AndroidRow, { title: 'Home music', subtitle: 'Leaving the library…' }, h('div', { className: 'bae-a-welcome-libraries-wait' }, h(AndroidSpinner))) : null,
        h(AndroidRow, { title: 'Odile’s records', button: true, off: removing }, h(AndroidMoreMenu, { off: removing })),
        s === 'damaged-join' ? h(AndroidRow, { title: 'Unfinished join that cannot be read', subtitle: '3b9e07d4-51a2…0f2d9a7c4e81' }, h(AndroidButton, { kind: 'text' }, 'Discard…')) : null,
        s === 'unreadable' ? h(AndroidRow, { title: 'Library that cannot be read', subtitle: '6f1c2a9e-4b7d…3d5e8b2f7c14' }, h(AndroidButton, { kind: 'text' }, 'Remove…')) : null,
        h(AndroidRow, { title: 'Add library', action: true, leadingIcon: 'add', off: removing })));
  }

  // The welcome window's content: the logo over "Your music, everywhere", then the libraries on this Mac (`state` is MacWelcomeLibraries') and what follows them (`children`, such as the restore codes iCloud Keychain holds). `empty`: this Mac has no library yet, so a larger logo sits over MacWelcomeActions.
  function MacWelcomeHome(props) {
    if (props.empty) {
      return h('div', { className: 'bae-m-welcome-home bae-m-welcome-home-empty' },
        h(Logo, { size: 120, bob: true, glow: true }),
        h('div', { className: 'bae-m-welcome-tagline' }, 'Your music, everywhere'),
        h('div', { className: 'bae-m-welcome-home-actions' }, h(MacWelcomeActions)));
    }
    return h('div', { className: 'bae-m-welcome-home' },
      h(Logo, { size: 72, bob: true, glow: true }),
      h('div', { className: 'bae-m-welcome-tagline' }, 'Your music, everywhere'),
      h('div', { className: 'bae-m-welcome-home-libraries' }, h(MacWelcomeLibraries, { state: props.state }), props.children));
  }

  // The welcome screen's content, as MacWelcomeHome's: `empty` puts AndroidWelcomeActions at the bottom of the screen under a larger logo.
  function AndroidWelcomeHome(props) {
    if (props.empty) {
      return h('div', { className: 'bae-a-welcome-home bae-a-welcome-home-empty' },
        h('div', { className: 'bae-a-welcome-home-brand' },
          h(Logo, { size: 150, bob: true, glow: true }),
          h('div', { className: 'bae-a-welcome-tagline' }, 'Your music, everywhere')),
        h(AndroidWelcomeActions));
    }
    return h('div', { className: 'bae-a-welcome-home' },
      h('div', { className: 'bae-a-welcome-home-logo' }, h(Logo, { size: 96, bob: true, glow: true })),
      h('div', { className: 'bae-a-welcome-tagline bae-a-welcome-tagline-centred' }, 'Your music, everywhere'),
      h(AndroidWelcomeLibraries, { state: props.state }),
      props.children);
  }

  var RESTORE_CODE = 'CVR1-UJZDEGXDNCF32EPF3DHODZDOCIS2JHTLGMXGEDN73U55XTPLPFT7V4SEH2KVJ72CEUVW75EFR6EDT4SYWB5WKH7DNSIPZZ7FK4ZRI3R2WYOJFLJOOA7LQSAJ2XUID5ZZZZG6ZDMEN4KHVDGAJGXBENYJQWX6HH7566TFJGVQ6KBNXJBTFQXKWOVOMPZOM7WBBR6QMW4WXFOGO6MVN6A6WFHYM6L3VFZ5ZFKKIBJ5J6WJIBAGI3MNB';
  var RESTORE_NOTE = 'Print this code or save it in a password manager. It gets you back into your library if you lose all your devices.';
  var RESTORE_S3_NOTE = 'You will also need your S3 access key and secret.';
  function flag(v) { return v === true || v === 'true'; }

  // The library's restore code in Settings: hidden but for its ends until the person shows it, with Print… and Copy beside it, and what the code is for under it. `shown` draws it shown; `s3` adds that an S3 library also needs its key.
  function MacRestoreCode(props) {
    var shownState = React.useState(null), copiedState = React.useState(false);
    var shown = shownState[0] != null ? shownState[0] : flag(props.shown);
    var copied = copiedState[0];
    var code = RESTORE_CODE;
    return h('div', { className: 'bae-m-restore' },
      h('div', { className: 'bae-m-restore-code' },
        shown ? h('span', { 'aria-label': 'Restore code', className: 'bae-m-restore-text' }, 'CVR1 ' + code.slice(5).match(/.{1,4}/g).join(' '))
          : h('span', { role: 'img', 'aria-label': 'Restore code, partly hidden', className: 'bae-m-restore-masked' },
            h('span', { 'aria-hidden': true, className: 'bae-m-restore-masked-head' }, code.slice(0, 9) + '•'.repeat(code.length - 13)),
            h('span', { 'aria-hidden': true, className: 'bae-m-restore-masked-tail' }, code.slice(-4))),
        h(MacButton, { onClick: function () { shownState[1](!shown); } }, shown ? 'Hide' : 'Show'),
        h(MacButton, null, 'Print…'),
        h(MacButton, { kind: 'icon', icon: copied ? 'check' : 'copy', label: copied ? 'Copied' : 'Copy', onClick: function () {
          try { navigator.clipboard.writeText(code); } catch (e) {}
          copiedState[1](true);
        } })),
      h(MacNote, null, RESTORE_NOTE),
      flag(props.s3) ? h(MacNote, null, RESTORE_S3_NOTE) : null);
  }

  function AndroidRestoreCode(props) {
    var shown = flag(props.shown);
    var text = shown ? 'CVR1 ' + RESTORE_CODE.slice(5).match(/.{1,4}/g).join(' ') : 'CVR1 UJZD •••• •••• •••• •••• •••• •••• •••• •••• •••• •••• •••• •••• 3MNB';
    return h('div', { className: 'bae-a-restore' },
      h(AndroidSection, { title: 'Restore code' }),
      h(AndroidCard, null, shown ? h('div', { className: 'bae-a-restore-code' }, text) : h('div', { role: 'img', 'aria-label': 'Restore code, partly hidden', className: 'bae-a-restore-code' }, text)),
      h('div', { className: 'bae-a-restore-actions' },
        h(AndroidButton, { kind: 'outlined' }, shown ? 'Hide' : 'Show code'),
        h(AndroidButton, { kind: 'outlined' }, 'Copy'),
        h(AndroidButton, { kind: 'outlined' }, 'Print')),
      h(AndroidNote, null, RESTORE_NOTE),
      flag(props.s3) ? h(AndroidNote, null, RESTORE_S3_NOTE) : null);
  }

  function deleteOldKey(oldKey) { return 'Delete the old key, ' + oldKey + ', in your S3 provider’s console.'; }

  var CLOUD_PROVIDER = { s3: 'S3-compatible', dropbox: 'Dropbox', onedrive: 'OneDrive', icloud: 'iCloud' };
  // The rows that say where a library's data is, as coven records it: the provider, then on S3 the endpoint, region and bucket, and on Dropbox or OneDrive the account and the folder when bae knows them (`account`, `folder`). iCloud has the provider alone.
  function cloudRows(props, provider) {
    var rows = [h(MacRow, { key: 'p', label: 'Provider', value: CLOUD_PROVIDER[provider] })];
    if (provider === 's3') {
      rows.push(h(MacRow, { key: 'e', label: 'Endpoint', value: props.endpoint || 'https://s3.home.lan' }),
        h(MacRow, { key: 'r', label: 'Region', value: props.region || 'us-east-1' }),
        h(MacRow, { key: 'b', label: 'Bucket', value: props.bucket || 'my-bucket' }));
    } else if (provider === 'dropbox' || provider === 'onedrive') {
      if (props.account) rows.push(h(MacRow, { key: 'a', label: 'Account', value: props.account }));
      if (props.folder) rows.push(h(MacRow, { key: 'f', label: 'Folder', value: props.folder }));
    }
    return rows;
  }

  // A library's cloud home in the Sync settings: the Cloud home heading over where its data is (`provider`: 's3', the default, 'dropbox', 'onedrive' or 'icloud'), then on S3 the access key this Mac uses (`keyId`) with Replace key…, and on Dropbox or OneDrive Open in browser for its folder. `oldKey`: the S3 key the person just replaced, which a notice under the box asks them to delete in their provider's console. `bare` draws the rows alone, for a box that holds other rows too, as the welcome window's library details do.
  function MacCloudHome(props) {
    var provider = props.provider || 's3';
    var rows = cloudRows(props, provider);
    if (props.bare) return h(React.Fragment, null, divided(rows));
    if (provider === 's3') rows.push(h(MacRow, { key: 'k', label: 'Access key ID', value: props.keyId || 'AKIA4EXAMPLEOLD' }, h(MacButton, null, 'Replace key…')));
    if ((provider === 'dropbox' || provider === 'onedrive') && props.folder) rows.push(h(MacRow, { key: 'o', label: '' }, h(MacButton, null, 'Open in browser')));
    return h('div', { className: 'bae-mac bae-m-cloudhome' },
      h(MacSection, { title: 'Cloud home' },
        h(MacBox, null, rows),
        provider === 's3' && props.oldKey ? h(MacNotice, { tone: 'info', dismiss: true, title: 'Saved the new S3 key.', subtitle: deleteOldKey(props.oldKey) }) : null));
  }

  function AndroidCloudHome(props) {
    return h('div', { className: 'bae-a-cloudhome' },
      h(AndroidSection, { title: 'Cloud home' }),
      h(AndroidCard, null,
        h(AndroidRow, { title: 'Provider', subtitle: 'S3-compatible' }),
        h(AndroidRow, { title: 'Endpoint', subtitle: 'https://s3.home.lan' }),
        h(AndroidRow, { title: 'Region', subtitle: 'us-east-1' }),
        h(AndroidRow, { title: 'Bucket', subtitle: 'my-bucket' }),
        h(AndroidRow, { title: 'Access key ID', subtitle: props.keyId || 'AKIA4EXAMPLEOLD' }),
        h(AndroidRow, { title: 'Replace key', button: true })),
      props.oldKey ? h('div', { className: 'bae-a-cloudhome-notice' }, h(AndroidNotice, { tone: 'info', dismiss: true, title: 'Saved the new S3 key.', subtitle: deleteOldKey(props.oldKey) })) : null);
  }

  // The two fields of an S3 access key, as Replace key and joining an S3 library ask for them. `focus`: 'id' or 'secret' (the default), the field that has focus; `refused`: S3 storage turned the key down, said above the fields.
  function MacS3KeyFields(props) {
    var focus = props.focus || 'secret';
    return h('div', { className: 'bae-mac bae-m-s3key' },
      flag(props.refused) ? h(MacNotice, { quiet: true, tone: 'danger', title: 'Could not use this key', subtitle: 'S3 storage does not accept this access key.' }) : null,
      h(MacTextField, { label: 'Access key ID', labelWidth: 120, value: props.keyId || '', state: focus === 'id' ? 'focused' : 'normal' }),
      h(MacTextField, { label: 'Secret access key', labelWidth: 120, value: props.secret || '', secret: true, state: focus === 'secret' ? 'focused' : 'normal' }));
  }

  function AndroidS3KeyFields(props) {
    var focus = props.focus || 'secret';
    return h('div', { className: 'bae-a-s3key' },
      flag(props.refused) ? h(AndroidNotice, { quiet: true, tone: 'danger', title: 'Could not use this key', subtitle: 'S3 storage does not accept this access key.' }) : null,
      h(AndroidTextField, { label: 'Access key ID', value: props.keyId || '', state: focus === 'id' ? 'focused' : 'normal' }),
      h(AndroidTextField, { label: 'Secret access key', value: props.secret ? '•'.repeat(String(props.secret).length) : '', state: focus === 'secret' ? 'focused' : 'normal' }));
  }

  // The S3 storage form for a new library: where the data goes and the key that reaches it, then Cancel and Set up. `error`: 'none', or what S3 storage refused: 'key' and 'denied' show above the form, 'region' under the region, and 'bucket' and 'occupied' under the bucket.
  function MacS3Form(props) {
    var error = props.error || 'none';
    var top = { key: 'This key wasn’t accepted. Check the access key ID and secret access key.', denied: 'This key cannot use this S3 storage. Give it read and write access, or use another key.' }[error] || '';
    var bucketError = { bucket: 'The cloud home was not found.', occupied: 'Use empty S3 storage or a folder that only this library uses.' }[error] || '';
    var regionError = error === 'region' ? 'The region or endpoint does not match this S3 storage.' : '';
    var field = function (label, value, err, secret) {
      return h(MacFieldRow, { label: label, labelWidth: 120 }, h(MacTextField, { value: value, state: err ? 'error' : 'normal', error: err, secret: secret }));
    };
    return h('div', { className: 'bae-m-s3form' },
      top ? h(MacNotice, { quiet: true, tone: 'danger', title: top }) : null,
      h(MacSection, { title: 'Location' },
        h(MacBox, null, field('Endpoint', 'https://s3.home.lan'), field('Region', 'us-east-1', regionError), field('Bucket', 'my-bucket', bucketError))),
      h(MacSection, { title: 'Key' },
        h(MacBox, null, field('Access key ID', 'a1b2c3d4e5f6'), field('Secret access key', 'abcdefghijkl', '', true))),
      h(MacSheetFooter, null, h(MacButton, null, 'Cancel'), h(MacButton, { kind: 'primary' }, 'Set up')));
  }

  // The folder picker for a Dropbox or OneDrive library: a list of folders to choose an empty one from. `provider`: 'dropbox' (shared folders, in one list) or 'onedrive' (folders the person opens, under a path with Up). `state`: 'list', 'naming' (typing a new folder's name), 'made' (bae made the folder and chose it), 'notEmpty' (the chosen folder holds files) or 'empty' (no folders). The setup sheet's MacSheetFooter, under it, holds New folder…, Look again, Cancel and Set up.
  function MacFolderPicker(props) {
    var dropbox = (props.provider || 'dropbox') === 'dropbox';
    var state = props.state || 'naming';
    var naming = state === 'naming';
    var names = dropbox ? ['Family', 'Music', 'Photos'] : ['Live', 'Albums', 'Mixes'];
    var picked = state === 'made' ? 'bae' : naming ? null : names[1];
    var folders = state === 'empty' ? [] : state === 'made' ? names.concat(['bae']) : names;
    return h('div', { className: 'bae-m-folderpicker' },
      h('div', { className: 'bae-m-folderpicker-heading' }, dropbox ? 'Choose an empty shared folder for this library.' : 'Choose an empty folder for this library.'),
      dropbox ? null : h('div', { className: 'bae-m-folderpicker-path' },
        h('div', { className: 'bae-m-folderpicker-trail' }, 'OneDrive › ', h('span', { className: 'bae-m-folderpicker-here' }, 'Music')),
        h(MacButton, { kind: 'borderless', neutral: true, off: naming }, 'Up')),
      h(MacBox, { empty: dropbox ? 'No shared folders yet' : 'No folders here yet' },
        folders.map(function (name) { return h(MacRow, { key: name, label: name, selected: name === picked, chevron: !dropbox }); }),
        naming ? h(MacFieldRow, { alignText: true }, h(MacTextField, { label: 'Folder name', labelHidden: true, value: 'bae', state: 'focused' }), h(MacButton, null, 'Create')) : null),
      state === 'notEmpty' ? h(MacNotice, { quiet: true, tone: 'danger', title: 'This folder is not empty. Choose an empty folder, or make a new one.' }) : null);
  }

  var INVITE_EMAIL = { dropbox: 'Dropbox email', onedrive: 'Microsoft account email', icloud: 'Apple Account email' };
  var INVITE_ACCOUNT = { dropbox: 'Dropbox account', onedrive: 'Microsoft account', icloud: 'Apple Account' };
  var INVITE_MINE = 'They will use your key. To remove them later, you make a new key for yourself and enter it on each of your devices.';
  var INVITE_THEIR_KEY = 'Their own key to the S3 storage, so they can join.';
  function inviteWhy(storage) { return 'The library is shared with this ' + INVITE_ACCOUNT[storage] + ' so they can join.'; }

  // The invite form's fields: the person's account email on Dropbox, OneDrive or iCloud, or on S3 the key they join with, then their role and how long the invite lasts. `storage`: 'dropbox', 'onedrive', 'icloud' or 's3' (the default); `keyOwner`: on S3, 'theirs' (their own key, in two fields) or 'mine' (they use yours).
  function MacInviteFields(props) {
    var storage = props.storage || 's3';
    var s3 = storage === 's3';
    var mine = s3 && (props.keyOwner || 'theirs') === 'mine';
    var lw = s3 ? 120 : 150;
    return h('div', { className: 'bae-mac bae-m-invitefields' },
      INVITE_EMAIL[storage] ? h(MacBox, null, h(MacFieldRow, { label: INVITE_EMAIL[storage], labelWidth: lw }, h(MacTextField, { label: INVITE_EMAIL[storage], labelHidden: true, value: 'dan@example.com', state: 'focused', note: inviteWhy(storage) }))) : null,
      s3 && !mine ? h(MacBox, null,
        h(MacFieldRow, { label: 'Access key ID', labelWidth: lw }, h(MacTextField, { label: 'Access key ID', labelHidden: true, value: 'AKIAEXAMPLEKEY', state: 'focused' })),
        h(MacFieldRow, { label: 'Secret access key', labelWidth: lw }, h(MacTextField, { label: 'Secret access key', labelHidden: true, value: 'abcdefghijkl', secret: true, note: INVITE_THEIR_KEY }))) : null,
      mine ? h(MacNote, null, INVITE_MINE) : null,
      h(MacBox, null,
        h(MacFieldRow, { label: 'Role', labelWidth: lw }, h(MacSegmentedControl, { options: 'Member,Admin', selected: 'Member' })),
        h(MacFieldRow, { label: 'Invite lasts', labelWidth: lw }, h(MacSegmentedControl, { options: '7 days,30 days', selected: '7 days' }))));
  }

  // As MacInviteFields', on Android, where a text button under the S3 key switches between their key and yours. `storage` has no 'icloud'.
  function AndroidInviteFields(props) {
    var storage = props.storage || 's3';
    var s3 = storage === 's3';
    var mine = s3 && (props.keyOwner || 'theirs') === 'mine';
    var email = storage === 'dropbox' || storage === 'onedrive' ? INVITE_EMAIL[storage] : '';
    return h('div', { className: 'bae-a-invitefields' },
      email ? h(AndroidTextField, { label: email, value: 'dan@example.com', state: 'focused', helper: inviteWhy(storage) }) : null,
      s3 && !mine ? h(AndroidTextField, { label: 'Access key ID', value: 'AKIAEXAMPLEKEY', state: 'focused' }) : null,
      s3 && !mine ? h(AndroidTextField, { label: 'Secret access key', value: '••••••••••••••••', helper: INVITE_THEIR_KEY }) : null,
      mine ? h('div', { className: 'bae-a-invitefields-mine' }, INVITE_MINE) : null,
      s3 ? h('div', { className: 'bae-a-invitefields-switch' }, h(AndroidButton, { kind: 'text', alignTo: mine ? 'text' : 'field' }, mine ? 'Use their own key instead' : 'Use my key instead')) : null,
      h(AndroidSegmentedControl, { label: 'Role', options: 'Member,Admin', selected: 'Member' }),
      h(AndroidSegmentedControl, { label: 'Invite lasts', options: '7 days,30 days', selected: '7 days' }));
  }

  // How the library syncs, in Settings: the status, in its tone, with what caused it (`cause`) and, while a wait counts down, the countdown under it (`countdown`), a button that fixes it (`fix`), then when this device last synced (`lastSynced`). With no cause and no fix, the status is a plain row; with either, its explanation sits under it in the same piece of the box.
  function MacSyncStatus(props) {
    var tone = props.tone;
    var more = !!(props.cause || props.fix);
    var head = more
      ? h('div', { className: 'bae-m-syncstatus-line' },
        h('span', { className: 'bae-m-syncstatus-label' }, h('span', { className: 'bae-m-syncstatus-title' }, h('span', null, 'Status'))),
        props.status ? h('span', { className: cx('bae-m-syncstatus-value', tone && 'bae-tone-' + tone) }, tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.status) : null)
      : h(MacRow, { label: 'Status', value: props.status, tone: tone });
    return h('div', { className: 'bae-mac' },
      h(MacBox, null,
        h('div', null,
          head,
          props.cause ? h('div', { className: 'bae-m-syncstatus-cause' }, props.cause, props.countdown ? h('div', { className: 'bae-m-syncstatus-countdown bae-num' }, props.countdown) : null) : null,
          props.fix ? h('div', { className: 'bae-m-syncstatus-fix' }, h(MacButton, { kind: 'primary' }, props.fix)) : null),
        props.lastSynced ? h(MacRow, { label: 'Last synced', value: props.lastSynced }) : null));
  }

  // As MacSyncStatus', on Android: when this device last synced is a line in the same tile, under the rest.
  function AndroidSyncStatus(props) {
    var tone = props.tone;
    var more = !!(props.cause || props.fix || props.lastSynced);
    var head = more
      ? h('div', { className: 'bae-a-syncstatus-line' },
        h('div', { className: 'bae-a-syncstatus-lines' },
          h('div', { className: 'bae-a-syncstatus-title' }, 'Status'),
          props.status ? h('div', { className: cx('bae-a-syncstatus-value', tone && 'bae-tone-' + tone) }, tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.status) : null))
      : h(AndroidRow, { title: 'Status', subtitle: props.status, tone: tone });
    return h(AndroidCard, null,
      h('div', { className: 'bae-a-syncstatus' },
        head,
        props.cause ? h('div', { className: 'bae-a-syncstatus-cause' }, props.cause, props.remedy ? h('div', { className: 'bae-a-syncstatus-remedy' }, props.remedy) : null) : null,
        props.fix ? h('div', { className: 'bae-a-syncstatus-fix' }, h(AndroidButton, null, props.fix)) : null,
        props.lastSynced ? h('div', { className: 'bae-a-syncstatus-when' }, 'Last synced ' + props.lastSynced) : null));
  }

  // The Save as panel's Format line: "Format:" and the format pop-up, centred under the panel's fields. `value`, `open`, `items` and `chosen` are MacPopUpButton's.
  function MacSavePanelFormat(props) {
    return h('div', { className: 'bae-m-savepanel-format' }, 'Format: ',
      h(MacPopUpButton, { value: props.value, width: 190, open: props.open, items: props.items, chosen: props.chosen }));
  }
