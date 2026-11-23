//! L'archive `.tar.zst` de Phonon-2, lue telle quelle, en mémoire, sans rien
//! écrire sur le disque : le `PhononContainer.extract` du Swift (`tar -xOf`
//! de macOS), en Rust pur (ruzstd, tar).
//!
//! L'archive vient d'un téléchargement. Tout le flux décompressé a un budget
//! (`MAX_ARCHIVE_BYTES`) : au-delà, une erreur, quelle que soit l'entrée qui
//! le dépasse (un membre ignoré, un nom long GNU, une extension PAX, que
//! `tar` lit en entier sans les rendre). Chaque entrée rendue est vérifiée
//! avant d'être lue (un nom qui sortirait de l'archive, une taille annoncée
//! démesurée : une erreur), un membre lu doit avoir tous ses octets, et
//! chaque trame zstd sa somme de contrôle quand elle en porte une. Rien
//! n'est extrait : un lien ou un dossier n'est jamais suivi.

use anyhow::{anyhow, bail, Result};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};
use std::cell::Cell;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Component, Path};
use std::rc::Rc;

/// Au-delà, une taille annoncée est refusée avant d'être lue. Le plus gros
/// membre de Phonon-2 (`model.fermion`) fait 177 438 361 octets.
pub const MAX_MEMBER_BYTES: u64 = 1 << 30;

/// Au-delà, l'archive décompressée (le tar entier, en-têtes compris) est
/// refusée, au fil de la lecture. Phonon-2 en fait environ 178 Mo : deux
/// membres au plafond, plus une marge pour les en-têtes.
pub const MAX_ARCHIVE_BYTES: u64 = 2 * MAX_MEMBER_BYTES + (64 << 20);

/// La capacité réservée d'avance pour un membre : le reste vient avec les
/// octets lus, jamais sur la seule foi de la taille annoncée.
const INITIAL_CAPACITY: u64 = 1 << 20;

/// Les membres `names` de l'archive, dans cet ordre, chacun entier. Un
/// membre absent, vide, présent deux fois ou qui n'est pas un fichier est
/// une erreur qui le nomme ; toute erreur nomme l'archive.
pub fn read_members<const N: usize>(archive: &Path, names: [&str; N]) -> Result<[Vec<u8>; N]> {
    read_members_within(archive, names, MAX_ARCHIVE_BYTES)
}

/// `read_members`, avec un budget de `budget` octets décompressés.
fn read_members_within<const N: usize>(
    archive: &Path,
    names: [&str; N],
    budget: u64,
) -> Result<[Vec<u8>; N]> {
    let shown = archive.display();
    let unreadable = |detail: &dyn Display| {
        anyhow!(crate::tr!(
            en = "unreadable Phonon archive ({shown}): {detail}",
            fr = "archive Phonon illisible ({shown}) : {detail}",
        ))
    };
    let file = File::open(archive).map_err(|err| unreadable(&err))?;
    let spent = Rc::new(Cell::new(false));
    // Une erreur de lecture : le budget dépassé, s'il l'est (`tar` peut
    // envelopper l'erreur du lecteur), sinon l'erreur elle-même.
    let fail = |detail: &dyn Display| {
        if spent.get() {
            anyhow!(crate::tr!(
                en = "Phonon archive ({shown}): more than {budget} bytes once decompressed",
                fr = "archive Phonon ({shown}) : plus de {budget} octets une fois décompressée",
            ))
        } else {
            unreadable(detail)
        }
    };
    let mut tar = tar::Archive::new(Budget {
        source: Zstd::new(BufReader::new(file)),
        left: budget,
        spent: Rc::clone(&spent),
    });
    let mut found: [Option<Vec<u8>>; N] = std::array::from_fn(|_| None);
    for entry in tar.entries().map_err(|err| fail(&err))? {
        let mut entry = entry.map_err(|err| fail(&err))?;
        let path = entry.path().map_err(|err| fail(&err))?.into_owned();
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
        // Au plus INITIAL_CAPACITY : la capacité tient dans un usize.
        let mut data = Vec::with_capacity(size.min(INITIAL_CAPACITY) as usize);
        (&mut entry)
            .take(size)
            .read_to_end(&mut data)
            .map_err(|err| fail(&format!("{name} : {err}")))?;
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
    io::copy(&mut tar.into_inner(), &mut io::sink()).map_err(|err| fail(&err))?;
    if let Some(slot) = found.iter().position(Option::is_none) {
        let name = names[slot];
        bail!(crate::tr!(
            en = "Phonon archive ({shown}): {name} is missing",
            fr = "archive Phonon ({shown}) : {name} manque",
        ));
    }
    Ok(found.map(Option::unwrap_or_default))
}

/// `source`, jusqu'à `left` octets : au-delà, une erreur, et `spent` le dit.
struct Budget<R> {
    source: R,
    left: u64,
    spent: Rc<Cell<bool>>,
}

impl<R: Read> Read for Budget<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Un octet de plus que le reste permis : de quoi voir le dépassement.
        let room =
            usize::try_from(self.left.saturating_add(1)).map_or(buf.len(), |r| r.min(buf.len()));
        let n = self.source.read(&mut buf[..room])?;
        if n as u64 > self.left {
            self.spent.set(true);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive : budget de décompression dépassé",
            ));
        }
        self.left -= n as u64;
        Ok(n)
    }
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

    /// Un nom long GNU (`L`) qui annonce 64 Gio : `tar` le lit en entier
    /// sans le rendre. Le budget l'arrête au fil de la lecture, avec une
    /// erreur qui nomme l'archive et le budget.
    #[test]
    fn a_gnu_long_name_beyond_the_budget_is_an_error() {
        let mut header = tar::Header::new_gnu();
        header.as_old_mut().name[..13].copy_from_slice(b"././@LongLink");
        header.set_size(64 << 30);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::GNULongName);
        header.set_cksum();
        let mut builder = tar::Builder::new(Vec::new());
        builder.append(&header, &vec![b'a'; 4 << 20][..]).unwrap();
        let tar = builder.into_inner().unwrap();
        let zst = ruzstd::encoding::compress_to_vec(
            tar.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        assert!(zst.len() < tar.len() / 10, "{} octets", zst.len());
        let path = archive_file("nom-long", &zst);
        let err = format!(
            "{:#}",
            read_members_within(&path, ["model.fermion"], 1 << 20).unwrap_err()
        );
        assert!(err.contains("1048576"), "{err}");
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    /// Un membre que personne ne demande, plus gros que le budget une fois
    /// décompressé : une erreur, pas une lecture sans fin.
    #[test]
    fn an_unasked_member_beyond_the_budget_is_an_error() {
        let big = vec![0u8; 2 << 20];
        let path = archive_file(
            "membre-ignore-enorme",
            &tar_zst(&[
                (b"config.json", b"{}", 2),
                (b"bombe.bin", &big, big.len() as u64),
                (b"model.fermion", b"x", 1),
            ]),
        );
        let err = format!(
            "{:#}",
            read_members_within(&path, ["config.json", "model.fermion"], 1 << 20).unwrap_err()
        );
        assert!(err.contains("1048576"), "{err}");
        assert!(err.contains(&path.display().to_string()), "{err}");
        // Sous le budget réel, la même archive se lit.
        let [config, model] = read_members(&path, ["config.json", "model.fermion"]).unwrap();
        assert_eq!(
            (config.as_slice(), model.as_slice()),
            (&b"{}"[..], &b"x"[..])
        );
    }
}
