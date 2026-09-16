# Cost discount ordering audit

Root source inspection at e9235605, independently checked by Astra, found an additional C3 gap beyond Flow/Repeat multiplicity. No gameplay reproduction or new test has run for this finding.

Pinned Core 356.4.c and 356.4.d.1 order component discounts before total discounts while allowing ordering within each class. Rule 356.4.e applies a minimum only to that discount. Its explicit example is Eager Apprentice plus Sky Splitter printed at 8 Energy with a 7-Might friendly unit: Eager first then Sky Splitter costs 0 Energy; the opposite order costs 1. Both are legal controller choices.

Current engine/cost.rs from_other_cards sums static discount values; discounts_of combines self, other-card and promise discounts before one subtraction. Eager Apprentice computes its minimum against base_of_item and counts earlier apprentices by physical card ID. Neither the cost representation nor the prompt flow can represent the stated ordering choice. Summing discounts may reproduce one result but does not implement the choice.

A12 acceptance in C3: structured discount operations with their component/total scope and individual minima; deterministic enumeration of meaningful legal orders; serialized controller choice before payment; complete-item quotes for normal and Limited plays. Test the explicit 0/1 example, multiple minimum discounts, optional additional cost/component ordering, saved-request continuation, refusal without consumption, and real native/hardened parity. Do not introduce a physical-ID or registry-order tie-break where the rules assign the choice to the controller. This does not reopen the bounded Economy recovery or add a blocker to the current Play port.
