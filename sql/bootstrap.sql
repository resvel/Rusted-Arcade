PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS "User" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "name" TEXT
);

CREATE TABLE IF NOT EXISTS "Rom" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "system" TEXT NOT NULL,
  "emulatorCore" TEXT,
  "slug" TEXT NOT NULL,
  "title" TEXT NOT NULL,
  "filePath" TEXT NOT NULL,
  "fileSize" INTEGER,
  "checksum" TEXT,
  "coverPath" TEXT,
  "previewVideoPath" TEXT,
  "previewPosterPath" TEXT,
  "previewDurationSec" INTEGER,
  "previewUpdatedAt" DATETIME,
  "addedAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS "ArcadeMetadata" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "romId" TEXT NOT NULL,
  "displayTitle" TEXT NOT NULL,
  "releaseYear" INTEGER,
  "manufacturer" TEXT,
  "genre" TEXT,
  "alternateTitlesJson" TEXT,
  "wikiPageKey" TEXT,
  "wikiTitle" TEXT,
  "wikiUrl" TEXT,
  "mameShortname" TEXT,
  "mameDescription" TEXT,
  "matchMethod" TEXT NOT NULL,
  "confidence" REAL NOT NULL,
  "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" DATETIME NOT NULL,
  CONSTRAINT "ArcadeMetadata_romId_fkey" FOREIGN KEY ("romId") REFERENCES "Rom" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "SaveState" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "profileId" TEXT NOT NULL,
  "romId" TEXT NOT NULL,
  "slot" INTEGER NOT NULL,
  "label" TEXT,
  "sizeBytes" INTEGER NOT NULL DEFAULT 0,
  "statePath" TEXT NOT NULL,
  "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" DATETIME NOT NULL,
  CONSTRAINT "SaveState_profileId_fkey" FOREIGN KEY ("profileId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "SaveState_romId_fkey" FOREIGN KEY ("romId") REFERENCES "Rom" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Favorite" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "profileId" TEXT NOT NULL,
  "romId" TEXT NOT NULL,
  "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Favorite_profileId_fkey" FOREIGN KEY ("profileId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "Favorite_romId_fkey" FOREIGN KEY ("romId") REFERENCES "Rom" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "GamepadMapping" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "profileId" TEXT NOT NULL,
  "system" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "vendorId" TEXT,
  "productId" TEXT,
  "mappingJson" TEXT NOT NULL,
  "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" DATETIME NOT NULL,
  CONSTRAINT "GamepadMapping_profileId_fkey" FOREIGN KEY ("profileId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS "Rom_slug_key" ON "Rom"("slug");
CREATE UNIQUE INDEX IF NOT EXISTS "Rom_filePath_key" ON "Rom"("filePath");
CREATE INDEX IF NOT EXISTS "Rom_title_nocase_idx" ON "Rom"("title" COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS "Rom_system_title_nocase_idx" ON "Rom"("system", "title" COLLATE NOCASE);
CREATE UNIQUE INDEX IF NOT EXISTS "ArcadeMetadata_romId_key" ON "ArcadeMetadata"("romId");
CREATE INDEX IF NOT EXISTS "ArcadeMetadata_displayTitle_idx" ON "ArcadeMetadata"("displayTitle");
CREATE INDEX IF NOT EXISTS "ArcadeMetadata_matchMethod_idx" ON "ArcadeMetadata"("matchMethod");
CREATE INDEX IF NOT EXISTS "SaveState_profileId_romId_idx" ON "SaveState"("profileId", "romId");
CREATE UNIQUE INDEX IF NOT EXISTS "SaveState_profileId_romId_slot_key" ON "SaveState"("profileId", "romId", "slot");
CREATE INDEX IF NOT EXISTS "Favorite_profileId_idx" ON "Favorite"("profileId");
CREATE INDEX IF NOT EXISTS "Favorite_romId_idx" ON "Favorite"("romId");
CREATE UNIQUE INDEX IF NOT EXISTS "Favorite_profileId_romId_key" ON "Favorite"("profileId", "romId");
CREATE INDEX IF NOT EXISTS "GamepadMapping_profileId_system_idx" ON "GamepadMapping"("profileId", "system");
CREATE UNIQUE INDEX IF NOT EXISTS "GamepadMapping_profileId_system_name_key" ON "GamepadMapping"("profileId", "system", "name");
