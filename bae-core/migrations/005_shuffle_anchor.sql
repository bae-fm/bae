-- The track fronted when shuffle is toggled on over a playing context, kept so
-- restore re-derives the fronted order (seed alone reproduces only the raw
-- permutation). NULL for sequential order and un-anchored shuffled orders.
ALTER TABLE playback_state ADD COLUMN shuffle_anchor TEXT;
