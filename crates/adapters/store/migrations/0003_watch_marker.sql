ALTER TABLE watched_folder ADD COLUMN marker_conflict TEXT;

ALTER TABLE watched_folder ADD COLUMN marker_dismissed TEXT NOT NULL DEFAULT '[]';
