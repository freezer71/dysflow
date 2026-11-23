//! L'archive `.tar.zst` de Phonon-2, lue telle quelle, en mémoire, sans rien
//! écrire sur le disque : le `PhononContainer.extract` du Swift (`tar -xOf`
//! de macOS), en Rust pur (ruzstd, tar).
//!
//! L'archive vient d'un téléchargement : chaque entrée est vérifiée avant
//! d'être lue (un nom qui sortirait de l'archive, une taille annoncée
//! démesurée : une erreur, avant toute allocation), un membre lu doit avoir
//! tous ses octets, et chaque trame zstd sa somme de contrôle quand elle en
//! porte une. Rien n'est extrait : un lien ou un dossier n'est jamais suivi.

use anyhow::{anyhow, bail, Result};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Component, Path};

/// Au-delà, une taille annoncée est refusée avant d'être lue. Le plus gros
/// membre de Phonon-2 (`model.fermion`) fait 177 438 361 octets.
pub const MAX_MEMBER_BYTES: u64 = 1 << 30;

/// Les membres `names` de l'archive, dans cet ordre, chacun entier. Un
/// membre absent, vide, présent deux fois ou qui n'est pas un fichier est
/// une erreur qui le nomme ; toute erreur nomme l'archive.
pub fn read_members<const N: usize>(archive: &Path, names: [&str; N]) -> Result<[Vec<u8>; N]> {
    let shown = archive.display();
    let unreadable = |detail: &dyn Display| {
        anyhow!(crate::tr!(
            en = "unreadable Phonon archive ({shown}): {detail}",
            fr = "archive Phonon illisible ({shown}) : {detail}",
        ))
    };
    let file = File::open(archive).map_err(|err| unreadable(&err))?;
    let mut tar = tar::Archive::new(Zstd::new(BufReader::new(file)));
    let mut found: [Option<Vec<u8>>; N] = std::array::from_fn(|_| None);
    for entry in tar.entries().map_err(|err| unreadable(&err))? {
        let mut entry = entry.map_err(|err| unreadable(&err))?;
        let path = entry.path().map_err(|err| unreadable(&err))?.into_owned();
        let name = path.display().to_string();
        if !path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!(crate::tr!(
                en = "Phonon archive ({shown}): the entry {name} points outside the archive",
                fr = "archive Phonon ({shown}) : l'entrée {name} pointe hors de l'archive",
            ));
        }
        let size = entry.size();
        if size > MAX_MEMBER_BYTES {
            bail!(crate::tr!(
                en = "Phonon archive ({shown}): the entry {name} announces {size} bytes, more than the {MAX_MEMBER_BYTES} allowed",
                fr = "archive Phonon ({shown}) : l'entrée {name} annonce {size} octets, plus que les {MAX_MEMBER_BYTES} permis",
            ));
        }
        let Some(slot) = names.iter().position(|n| Path::new(n) == path) else {
            continue;
        };
        if !entry.header().entry_type().is_file() {
            bail!(crate::tr!(
                en = "Phonon archive ({shown}): {name} is not a file",
                fr = "archive Phonon ({shown}) : {name} n'est pas un fichier",
            ));
        }
        if found[slot].is_some() {
            bail!(crate::tr!(
                en = "Phonon archive ({shown}): {name} appears twice",
                fr = "archive Phonon ({shown}) : {name} y est deux fois",
            ));
        }
        // `size` ≤ MAX_MEMBER_BYTES : la capacité tient dans un usize.
        let mut data = Vec::with_capacity(size as usize);
        (&mut entry)
            .take(size)
            .read_to_end(&mut data)
            .map_err(|err| unreadable(&format!("{name} : {err}")))?;
        if data.len() as u64 != size {
            bail!(crate::tr!(
                en = "truncated Phonon archive ({shown}): {name} has {} bytes out of {size}",
                fr = "archive Phonon tronquée ({shown}) : {name} a {} octets sur {size}",
                data.len(),
            ));
        }
        if data.is_empty() {
            bail!(crate::tr!(
                en = "Phonon archive ({shown}): {name} is empty",
                fr = "archive Phonon ({shown}) : {name} est vide",
            ));
        }
        found[slot] = Some(data);
    }
    // Le reste du flux, jusqu'au bout : chaque trame se termine et sa somme
    // de contrôle est vérifiée, même après le dernier membre lu.
    io::copy(&mut tar.into_inner(), &mut io::sink()).map_err(|err| unreadable(&err))?;
    if let Some(slot) = found.iter().position(Option::is_none) {
        let name = names[slot];
        bail!(crate::tr!(
            en = "Phonon archive ({shown}): {name} is missing",
            fr = "archive Phonon ({shown}) : {name} manque",
        ));
    }
    Ok(found.map(Option::unwrap_or_default))
}

/// Les trames zstd de `source`, l'une après l'autre (`zstd -T` en écrit
/// plusieurs), chacune jusqu'à sa fin, sa somme de contrôle vérifiée. Un flux
/// sans aucune trame, ou coupé dans une trame, est une erreur.
struct Zstd<R> {
    source: R,
    frame: FrameDecoder,
    open: bool,
    frames: usize,
}

impl<R: BufRead> Zstd<R> {
    fn new(source: R) -> Self {
        Self {
            source,
            frame: FrameDecoder::new(),
            open: false,
            frames: 0,
        }
    }

    /// La trame courante est lue en entier : sa somme de contrôle, si elle en
    /// porte une, doit être celle des octets rendus.
    fn close(&mut self) -> io::Result<()> {
        self.open = false;
        match self.frame.get_checksum_from_data() {
            Some(expected) if self.frame.get_calculated_checksum() != Some(expected) => {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "zstd : somme de contrôle fausse",
                ))
            }
            _ => Ok(()),
        }
    }
}

impl<R: BufRead> Read for Zstd<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.open {
                if self.frame.can_collect() > 0 {
                    return self.frame.read(buf);
                }
                if !self.frame.is_finished() {
                    self.frame
                        .decode_blocks(&mut self.source, BlockDecodingStrategy::UptoBlocks(1))
                        .map_err(io::Error::other)?;
                    continue;
                }
                self.close()?;
            }
            if self.source.fill_buf()?.is_empty() {
                if self.frames == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "zstd : aucune trame",
                    ));
                }
                return Ok(0);
            }
            self.frame
                .reset(&mut self.source)
                .map_err(io::Error::other)?;
            self.open = true;
            self.frames += 1;
        }
    }
}

/// L'archive, sans poids : des archives fabriquées ici. Les outils de
/// fabrication servent aussi aux essais du modèle (`phonon::tests`).
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::stt::mlx::phonon::ARCHIVE;
    use std::path::PathBuf;

    /// Un tirage reproductible (les essais Swift tirent au hasard ; ici, une
    /// suite fixe, pour qu'un échec se rejoue).
    pub(in crate::stt::mlx::phonon) struct Lcg(pub u64);

    impl Lcg {
        pub fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        /// Dans [−1, 1[.
        pub fn unit(&mut self) -> f32 {
            (self.next() % 2_000_000) as f32 / 1_000_000.0 - 1.0
        }
    }

    /// Une archive tar de `entries` (en-têtes ustar, tels que donnés),
    /// compressée en une trame zstd.
    pub(in crate::stt::mlx::phonon) fn tar_zst(entries: &[(&[u8], &[u8], u64)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for &(name, data, size) in entries {
            let mut header = tar::Header::new_ustar();
            // Le nom tel quel, sans les vérifications de `set_path` (`..` compris).
            header.as_old_mut().name[..name.len()].copy_from_slice(name);
            header.set_size(size);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder.append(&header, data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        ruzstd::encoding::compress_to_vec(
            tar.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        )
    }

    pub(in crate::stt::mlx::phonon) fn archive_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = crate::paths::test_scratch(&format!("stt-mlx-phonon-{name}"));
        let path = dir.join(ARCHIVE);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// Des octets peu compressibles, pour qu'une coupe tombe dans les données.
    pub(in crate::stt::mlx::phonon) fn noise(len: usize) -> Vec<u8> {
        let mut rng = Lcg(7);
        (0..len).map(|_| rng.next() as u8).collect()
    }

    #[test]
    fn the_members_are_read_whole_in_the_order_asked() {
        let (a, b) = (noise(5_000), b"{}".to_vec());
        let path = archive_file(
            "membres",
            &tar_zst(&[
                (b"autre.txt", b"x", 1),
                (b"model.fermion", &a, a.len() as u64),
                (b"config.json", &b, b.len() as u64),
            ]),
        );
        let [config, model] = read_members(&path, ["config.json", "model.fermion"]).unwrap();
        assert_eq!((config, model), (b, a));
    }

    /// Deux trames zstd à la suite (`zstd -T`) se lisent l'une après l'autre ;
    /// une somme de contrôle fausse est refusée.
    #[test]
    fn the_frames_follow_each_other_and_their_checksums_are_checked() {
        let data = noise(3_000);
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_ustar();
        header.set_path("config.json").unwrap();
        header.set_size(data.len() as u64);
        header.set_cksum();
        builder.append(&header, data.as_slice()).unwrap();
        let tar = builder.into_inner().unwrap();
        let (first, second) = tar.split_at(1_000);
        let level = ruzstd::encoding::CompressionLevel::Fastest;
        let mut bytes = ruzstd::encoding::compress_to_vec(first, level);
        bytes.extend(ruzstd::encoding::compress_to_vec(second, level));
        let [read] = read_members(&archive_file("trames", &bytes), ["config.json"]).unwrap();
        assert_eq!(read, data);

        // La somme de contrôle est le dernier mot de la trame.
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        let path = archive_file("somme-fausse", &bytes);
        let err = format!("{:#}", read_members(&path, ["config.json"]).unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    /// Une archive coupée : une erreur qui nomme l'archive, jamais un membre
    /// lu à moitié.
    #[test]
    fn a_truncated_archive_is_an_error() {
        let data = noise(20_000);
        let mut bytes = tar_zst(&[(b"model.fermion", &data, data.len() as u64)]);
        bytes.truncate(bytes.len() / 2);
        let path = archive_file("tronquee", &bytes);
        let err = format!("{:#}", read_members(&path, ["model.fermion"]).unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
        assert!(
            err.contains("illisible")
                || err.contains("unreadable")
                || err.contains("tronquée")
                || err.contains("truncated"),
            "{err}"
        );
    }

    /// Un membre plus court que sa taille annoncée, dans un flux zstd
    /// entier (le tar coupé avant d'être compressé) : une erreur qui le
    /// nomme, jamais ses octets.
    #[test]
    fn a_member_shorter_than_announced_is_an_error() {
        let data = noise(100);
        let path = archive_file(
            "membre-court",
            &tar_zst(&[(b"model.fermion", &data, 5_000)]),
        );
        let err = format!("{:#}", read_members(&path, ["model.fermion"]).unwrap_err());
        assert!(
            err.contains("model.fermion") && err.contains("5000"),
            "{err}"
        );
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    /// Une entrée `../` : refusée, même si elle n'est pas un membre lu.
    #[test]
    fn an_entry_outside_the_archive_is_an_error() {
        let path = archive_file(
            "hors-archive",
            &tar_zst(&[(b"../config.json", b"{}", 2), (b"config.json", b"{}", 2)]),
        );
        let err = format!("{:#}", read_members(&path, ["config.json"]).unwrap_err());
        assert!(err.contains("../config.json"), "{err}");
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    /// Une taille annoncée démesurée (1 Tio, sans les données) : refusée avant
    /// toute lecture ou allocation.
    #[test]
    fn an_absurd_announced_size_is_an_error() {
        let path = archive_file(
            "taille-demesuree",
            &tar_zst(&[(b"model.fermion", b"", 1 << 40)]),
        );
        let err = format!("{:#}", read_members(&path, ["model.fermion"]).unwrap_err());
        assert!(err.contains("model.fermion"), "{err}");
        assert!(err.contains(&(1u64 << 40).to_string()), "{err}");
        assert!(err.contains(&MAX_MEMBER_BYTES.to_string()), "{err}");
    }

    #[test]
    fn a_missing_member_is_an_error_that_names_it() {
        let path = archive_file("membre-absent", &tar_zst(&[(b"config.json", b"{}", 2)]));
        let err = format!(
            "{:#}",
            read_members(&path, ["config.json", "model.fermion"]).unwrap_err()
        );
        assert!(err.contains("model.fermion"), "{err}");
    }
}
