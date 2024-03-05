// -*- coding: utf-8 -*-
// ------------------------------------------------------------------------------------------------
// Copyright © 2023, stack-graphs authors.
// Licensed under either of Apache License, Version 2.0, or MIT license, at your option.
// Please see the LICENSE-APACHE or LICENSE-MIT files in this distribution for license details.
// ------------------------------------------------------------------------------------------------

use camino::Utf8Path;
use zip::ZipArchive;

use crate::builders::RelativePathBuilder;
use crate::Snapshot;
use crate::ID;

/// An error that occur while building a [`Snapshot`] from a zip archive.
#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    RelativePathBuilderError(#[from] crate::builders::RelativePathBuilderError),
    #[cfg(feature = "zip")]
    #[error("error reading zip archive")]
    ZipError(#[from] zip::result::ZipError),
}

impl Snapshot {
    /// Generates a snapshot from the contents of a zip archive.
    pub fn from_zip_archive<R>(archive: &mut ZipArchive<R>) -> Result<Snapshot, ZipError>
    where
        R: std::io::Read + std::io::Seek,
    {
        let mut builder = RelativePathBuilder::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if !file.is_file() {
                continue;
            }
            let file_id = ID::generate_file_id_from_reader(&mut file)?;
            let full_path = file
                .enclosed_name()
                .ok_or_else(|| zip::result::ZipError::InvalidArchive("invalid filename"))?;
            let full_path = Utf8Path::from_path(full_path)
                .ok_or_else(|| zip::result::ZipError::InvalidArchive("invalid filename"))?;
            builder.add_file(full_path, file_id)?;
        }

        let snapshot = builder.build()?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    #[test]
    fn can_create_snapshot_from_zip_file() {
        let zip_file = include_bytes!("../../data/typescript_minimal_project.zip");
        let mut zip_archive = ZipArchive::new(std::io::Cursor::new(zip_file)).unwrap();
        let snapshot = Snapshot::from_zip_archive(&mut zip_archive).unwrap();
        assert_eq!(
            indoc! {"
              root v0:f5b614e205e6460141de0ed1c5b6f44e068ad45536fbd2e1d73bf35ec9ad79e6

              tree v0:4d911f27db098222ba3f7d79387402c72445ed746f4496a635ccdd22a4973804
                index.ts file v0:43ceacd553f2c5e7be548a70a4bbfc2c8054d7a8f3fd91cc6e45c7e3245f7e01
                package.json file v0:afdb91e4840371d53ec63516ba6617b5a398d38155b6745bcc4dcd5ff3427404
                tsconfig.json file v0:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356
                util.ts file v0:803025895ea4cca0053f42d425944ced1d5583184eb4e334f6b231fdf479fae5

              tree v0:f5b614e205e6460141de0ed1c5b6f44e068ad45536fbd2e1d73bf35ec9ad79e6
                typescript_minimal_project tree v0:4d911f27db098222ba3f7d79387402c72445ed746f4496a635ccdd22a4973804
            "},
            snapshot.render().to_string(),
        );
    }
}
