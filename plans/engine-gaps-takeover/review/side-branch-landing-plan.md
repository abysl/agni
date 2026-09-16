# Existing side-branch landing plan

Read-only Astra disposition for main `20af74aa`, integration `28a77655`, Triggers `3e00abad`, and neutral effect groups `8e4e9094`. No builds, production edits, or agents launched. The current user authorization permits finishing and landing existing branch work; the broader unstarted roadmap remains paused. Agni determinism and neutral engine/plugin boundaries remain authoritative.

## 1. Promote the reviewed baseline first

**No newly identified source blocker prevents promoting integration independently of Triggers or neutral effect groups.** Root verified production is unchanged since reviewed Damage integration `3973e4bf`; commits through `28a77655` are planning/evidence changes. Root now reports fresh Riftbound 4,494 library + 10 blob compatibility + 10 match-state passes; Kai, lint and fresh artifacts/replay are being checked independently. This note does not claim those unfinished gates passed.

Main and integration share `3d7d5f33`; main has the additional roadmap-only commit `20af74aa`. This is a normal merge, not a fast-forward from current main. Preserve main's bounded-milestone policy while updating its release table with what actually lands. Resolve any roadmap conflict around the current user authorization: existing branch completion is active, the wider campaign is paused. Do not replace main's tree or discard planning history.

Root should finish the relevant fresh Kai, warning-denied lint/format and native/hardened replay gates, preserve named pre-merge refs, merge integration into local main, and record the resulting hash and tested artifact hashes. A source-identical post-merge tree does not require repeating expensive suites merely because documentation merged; any substantive conflict resolution does.

This promotion includes Rules/Statics, A9 owned cost grants, Economy, Play, the first three Costs clusters, Damage, the match-state foundation and reviewed Kai changes. It does not include the Triggers v14 port, neutral ABI/journal, fourth Costs additional settlement, or complete rematch UI/session behavior. Do not report those as shipped.

## 2. Finish Triggers as the smaller independent landing

The previous concrete production blockers have corresponding bounded repairs in `a2f0d87d`: batches larger than one no longer elide controller ordering through pointer equality; `record_excess` passes its captured assigner to `lethal_for`; Lent activation discounts use the complete item; excess rows sort canonically and reject duplicate keys. `8e9ae562` restores the meaningful wearer/static baseline regression. These changes preserve the baseline rather than completing A3/A4 identity work.

`f4099f00` adds actual combat assignment controls for Elder Dragon against a higher-Might defender, finite prevention plus Lotus multiplication, and both assigners. `3e00abad` adds blob decode, reconstructed registry and retained-item comparisons in Forge/Gardens/Heimerdinger/Warmog tests. Credit those precise guarantees: the tests clone the SDK table and call native helpers; they are not fresh HostSession/engine snapshots or hardened replay.

Required landing checkpoints:

1. Finish a bounded final source/test review of these fixes and the v14 schema against the already documented five-cluster scope. Keep all original saved regressions or explicit replacements; retain the restored RELIC wearer test. Do not count obsolete negative assertions removed in favor of positive tests as lost behavior.
2. Preserve independent old v12/v13 bodies, with nontrivial Pending/Noted and existing Limited/mode/Repeat/awaiting data, and assert explicit defaults versus retained fields. The independent v14 Granted/Lent/Noted/xd vector added in `a2f0d87d` is useful and closes the absence of an authored new-version vector; it does not replace full old-layout compatibility evidence. Preserve Damage v13 seat/card rows and A9 owned costs. Do not fabricate old bodies by changing only the version byte.
3. Add/run a small real registered Granted/Lent native + freshly hardened corpus. A practical sequence is Warmog equipped on Vi at Gardens: normal equip/payment and move/conquer creates the Granted buff item; save/reload before resolution and assert wearer/source/buff. On a later normal turn, activate the Gardens ability borrowed by the holder, save/reload its Lent item, and assert the holder's exhaust and XP outcome. Use real actions and exact costs; no injected queued item, attachment or trigger. Keep opponents' hands nonempty where priority must remain observable. If split into two independent games, that is acceptable and easier to diagnose.
4. Merge current promoted main into the candidate before final combined gates. Run the relevant package/compatibility, fresh native/hardened corpus, lint/format and Kai gates on the combined production tree, then merge Triggers to main and record blob v14 explicitly.

No need to start the other twenty trigger clusters, A3 captured-event redesign, A4 stable identity/once ledger, or D1 callback continuation migration to land this bounded recovery. The retained dynamic activation identity limitations remain documented debt.

## 3. Finish neutral SDK and harness in narrow checkpoints

Engine-half source approval stands at `4d292547`, boundary tests `8ac7a468`/`af477d9d`, plus reviewed `76335cb7` PublicReveal-zone correction. It is not whole-feature approval. Current worktree has a dirty SDK effect_group.rs edit owned by Luna; this review uses committed `8e4e9094` only.

The WIP commit is materially newer than the resumed review at `e4f330c6`: `preflight_state` now tracks duplicate nested cards, checks their first advertised count, tracks duplicate outer seats, and requires seats to be present. Do not re-report those exact guards as still absent. It also adds borrowed-preflight tests. However these fixtures commonly use null card/row placeholders, and exact-cap controls often assert only preflight success. They do not establish a valid parseable current/capsule state, actual unique roster membership, or engine/SDK equality. WIP remains unapproved pending completion and independent evidence.

Ordered SDK repair acceptance:

1. Finish borrowed preflight using the actual validated roster and duplicate policy, current and rollback capsule independently. Require complete valid exact-cap controls plus intentional over-cap payloads for cards, counters, aggregate/outer annotations, tokens, revealed, owed, shown and roster-derived Peeks. Vary state/group and roster/collection ordering. Duplicate/missing roster and oversized-first/duplicate-cards cases must reject before projection allocation. Preserve valid nongroup over-feature-cap controls. If a raw-cap vector deliberately uses otherwise invalid duplicate/dangling rows, label its limited purpose and pair it with a separately valid control.
2. Resolve incoming Action::Move versus Effect::Move bookkeeping: engine admission rejects positional no-op incoming moves, while real no-op effects preserve disclosure. Do not claim a fake incoming no-op Request proves an admitted-log divergence. Test helper contracts separately and test actual native admission.
3. Add direct started-game Join checks (sparse seat and 255→256 boundary) and the engine-valid out-of-range counter-start/clamping + rollback-row-absence case. Verify that public game entry helpers enforce dense-roster restrictions while the neutral SDK preserves sparse `{0,7}` and full 256-member rosters. Keep prior rollback/Peek/disclosure repairs and token shedding controls.
4. Submit coherent commits with per-finding failing-before/fixed-after evidence or precise already-fixed regression assertions. Review source before final suite counts are treated as acceptance.

Harness work must follow the existing frozen protocol, not revise it to fit a shallow test:

A. Build one reusable native driver from the **actual fixture verdict**: authoritative DecideRequest, SDK parse/from_decide, incremental effect application and project_verdict, then real engine fold. Compare full normalized projection including mechanical capsule and ordered information records, not merely group id/cards. Assert independent expected outcomes and plugin-state behavior; do not hand-copy a second effect list.

B. Add real mechanics and cold suffixes: actual relocation, annotations/exhaustion, existing clamped counter plus previously absent row, original annotated token destroyed and restored, a live newly allocated transient, state-shedding movement, rollback and post-terminal spawn proving accepted allocation high-water. Save an active group and recreate engine and plugin instances before the suffix. Rejected tentative requests must not consume allocation.

C. Add audience/roster/adversarial vectors: ordered/coalesced and later RequestReveal debt, hidden round trips, baseline/later Peek authorization under Owner visibility, shown pruning, original versus transient owed objects, `{0,7}` and 256 seats with real Counter/Peek operations, absent seats, Shared effective seat, invalid membership/allocator/duplicates, per-effect atomic caps, marker ordering/ownership, ABI0 guards and legacy snapshot migration. Assert actual owner/opponent views, and that decide state copies omit embedded group state as required.

D. Run the same decisive neutral corpus through fresh hardened engine + fixture plugin exports, new instances after restore, and real HostSession/client disclosure delivery. The phase-two five host/one net native tests are scaffolding, not this matrix; merely building fresh Wasm is not an execution result. No shipped card content is needed for neutral vectors. Compile ABI1 consumers and direct game Request builders; then perform the relevant merged Riftbound/Kai compatibility gates once on fresh artifacts.

Land neutral only after SDK source review and this cross-layer acceptance. Merge current main into its candidate first, preserving Triggers v14 if already landed; neutral engine snapshot/ABI versions and Riftbound blob versions are independent. Watch overlapping ctx/lib entry/roster guards, Request builders, state readers and net corpus helpers. Merge ancestry, never apply a direct side-branch-vs-main tree diff that would delete later integration documentation.

## 4. Explicit disposition of saved fourth Costs cluster

**Reject the original `d0639152` implementation as a landing candidate; preserve its commit, test inventory and corrected port design.** It pays additional costs before target selection and can treat parked kill/reveal work as payment completion. It is superseded by the recorded declaration/settlement design, not an omitted safe cherry-pick. Record first three Costs clusters as landed and the fourth as unimplemented; do not claim the neutral journal completes it.

This is the smallest honest disposition under the pause of broader unstarted work. A corrected fourth-cluster implementation is a separately named payment milestone, not a prerequisite to baseline/Triggers/neutral landing. Its dependency contract remains explicit: choose optional costs in declaration, select locations/targets, calculate full-item costs, then settle nonstandard/resource payments and final legality; preserve Ignored optional choices at zero cost and Free/base-cost distinctions. Respect 357.3 target-versus-sacrifice selection.

When that milestone is activated, it must include a serialized cost-owned cursor for the entire plan (including Spend::Kill/Gold), true reveal/wait/move continuations, plugin-only leaf rules undo and captured pending work, and neutral group completion/rollback. Never set paid while a replacement is parked, replay an accepted payment prefix, persist private view faces, rewind accepted IDs/log sequence, or use Reveal as a hidden-face setter. Choose a blob version after the then-current v14 and preserve existing chain/seat/card layouts and occupied tags. The original Legion Quartermaster/Meditation/Nami/Pyke/Rampage/Ruthless Strike/Sacrifice/Sea Monkey/Stalking Wolf/Zaun Punk/Zed tests remain mapped to this explicitly pending corrected milestone.

If the user's intended meaning of “finish existing work” includes delivering this corrected behavior now, root should include that named milestone explicitly in the current execution ledger and carry it through those dependencies. It cannot be marked completed by rejecting the unsafe original implementation; rejection only closes the question of whether to merge that saved code. No broad A1–A12 campaign or unrelated new clusters are implied.

## Completion record

For each landing, record source hash, main merge hash, reviewed scope, exact schema/ABI versions, meaningful tests and fresh artifact identities. Keep remaining debt separate from regressions introduced by that landing. Finish with main clean, preserved original/WIP refs, and an explicit account of accepted, superseded and still-paused behavior; never imply all original roadmap work is complete.
