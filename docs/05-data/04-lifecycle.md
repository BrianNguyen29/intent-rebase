# Entity Lifecycle

## Intent lifecycle
draft -> active -> superseded -> archived

## Diff lifecycle
pending -> computed -> reviewed -> accepted_for_rebase

## Rebase plan lifecycle
preview -> awaiting_confirmation -> approved -> applied -> verified -> closed
                                         \-> rejected

## Artifact lifecycle
generated -> active -> review_required -> invalid -> quarantined -> superseded

## Approval lifecycle
issued -> active -> stale -> revalidated|revoked|expired

## Side effect lifecycle
planned -> dispatched -> executed -> compensated|waived|incident-linked

## Checkpoint lifecycle
created -> eligible -> consumed -> superseded
