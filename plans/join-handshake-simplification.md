# Join handshake simplification — conclusions

Decided 2026-08-21 in conversation. Research basis: read of
coven sync/store/device_join/ (note: read at 7c359a48; re-verify
line refs against current main before executing).

## Design stance

A coven store is a small tight group. One device approves a join and
grants storage access. There is no split between an "owner" role and a
"storage admin" role on different machines. The split-role path cannot
work for a synchronous join anyway: the second machine has no way to
learn a join started short of polling the mailbox forever.

## Decisions

1. REMOVE the split owner/admin role from the join protocol.
   - Delete slot kinds ProvisionalBootstrap and
     ProviderAdmissionCompletion (they exist only to hand off between
     the two machines).
   - Delete device_join_transport_roles and the role comparison
     (authorization.rs ~182-185 at time of reading).
   - One party admits: it answers the access request, prepares the
     grant, and signs the approval.

2. DELETE the two write-only slot kinds: ProviderAdminTerminal and
   CleanupReceipt. Nothing reads them. They are in-process values that
   were given cloud writes for vocabulary symmetry.

3. MERGE Abandonment and Cancellation into one owner-terminated slot.
   Same event (the owner ended the join), split today by when it
   happened. The joiner's main wait already probes the abandonment
   slot on every look, so the merged slot removes the separate
   cancellation watch loop and its reads entirely.

4. TEARDOWN BY PREFIX LISTING. delete_attempt_slots probes all 15
   names today. List the attempt folder instead: one request gives the
   true set. Keep the read-before-delete byte-identity check per
   object. This also fixes a silent leak: files outside the known
   names are never deleted today.

## What stays and why

- Named slots stay. A numbered per-direction stream breaks addressing
  on Google Drive (opaque ids; a reader cannot derive a slot name) and
  makes crash resume harder (a resumer must first discover its index).
  A name is also a type declaration; wrong-slot artifacts fail fast.
- The 6 forward messages stay. They are the protocol floor.
- Create-once semantics stay (liveness: a stranger cannot overwrite a
  good artifact; signatures already handle forgery).
- The republish-on-collision rule stays (crash resume).
- The fingerprint needs nothing here: apps compute it from material
  both sides already hold; coven carries no slot for it.

## Expected outcome

Slot kinds 15 → 8 (six forward + activation + owner-terminated +
joiner-terminal + cleanup-activation = 9 kinds defined; a typical join
allocates 7). No-wait join ~57 provider requests → ~33.
Same-principal joins unaffected in behavior; all four behaviors kept:
cancel visibility, crash resume, forgery resistance, fingerprint
material.

## Execution order (each a separate gated push to coven main)

1. Split-role removal (decision 1).
2. Write-only slot deletion (decision 2).
3. Terminal merge (decision 3).
4. Prefix teardown (decision 4).

Open question queued behind these: how the joiner gets its initial
write access to the mailbox before the grant (read the pairing-code
bootstrap; unverified as of this writing).
