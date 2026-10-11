use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use mivia_domain::update::{Channel, Offer};
use mivia_ports::server::ServerVersion;
use mivia_ports::update::{Activity, UpdateEnv, UpdateError, Updater};
use parking_lot::Mutex;
use ring::signature::{Ed25519KeyPair, KeyPair as _};

pub struct TestUpdateKey {
    seed: [u8; 32],
    id: [u8; 8],
}

pub const TEST_UPDATE_KEY_A: TestUpdateKey = TestUpdateKey {
    seed: [
        164, 140, 170, 196, 127, 132, 216, 201, 9, 213, 73, 28, 6, 35, 242, 203, 155, 24, 196, 186,
        198, 16, 239, 29, 10, 182, 31, 130, 144, 200, 73, 252,
    ],
    id: [157, 60, 93, 27, 123, 116, 158, 159],
};

pub const TEST_UPDATE_KEY_B: TestUpdateKey = TestUpdateKey {
    seed: [
        24, 131, 17, 220, 34, 47, 125, 1, 165, 39, 110, 46, 142, 123, 226, 129, 23, 204, 118, 199,
        93, 82, 60, 49, 195, 162, 57, 96, 202, 97, 192, 33,
    ],
    id: [173, 215, 111, 215, 242, 49, 165, 229],
};

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .rev()
        .flat_map(|byte| [byte >> 4, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .map(|digit| digit.to_ascii_uppercase())
        .collect()
}

impl TestUpdateKey {
    fn pair(&self) -> Option<Ed25519KeyPair> {
        Ed25519KeyPair::from_seed_unchecked(&self.seed).ok()
    }

    pub fn public(&self) -> Option<String> {
        let pair = self.pair()?;
        let mut bin = b"Ed".to_vec();
        bin.extend_from_slice(&self.id);
        bin.extend_from_slice(pair.public_key().as_ref());
        let file = format!(
            "untrusted comment: minisign public key: {}\n{}\n",
            hex(&self.id),
            STANDARD.encode(bin)
        );
        Some(STANDARD.encode(file))
    }

    pub fn sign(&self, data: &[u8], trusted_comment: &str) -> Option<String> {
        let pair = self.pair()?;
        let signature = pair.sign(data);
        let mut bin = b"Ed".to_vec();
        bin.extend_from_slice(&self.id);
        bin.extend_from_slice(signature.as_ref());
        let mut global = signature.as_ref().to_vec();
        global.extend_from_slice(trusted_comment.as_bytes());
        let file = format!(
            "untrusted comment: signature from test key\n{}\ntrusted comment: {trusted_comment}\n{}\n",
            STANDARD.encode(bin),
            STANDARD.encode(pair.sign(&global).as_ref())
        );
        Some(STANDARD.encode(file))
    }
}

pub fn trusted_comment(version: Option<&str>) -> String {
    let mut comment = "timestamp:1700000000\tfile:MiViA.Desktop_x64-setup.exe".to_owned();
    if let Some(version) = version {
        comment.push_str("\tversion:");
        comment.push_str(version);
    }
    comment
}

#[derive(Default)]
pub struct FakeUpdater {
    pub offer: Mutex<Option<Offer>>,
    pub download_error: Mutex<Option<UpdateError>>,
    pub checks: Mutex<Vec<Channel>>,
    pub downloads: Mutex<Vec<String>>,
    pub installs: Mutex<Vec<String>>,
}

impl FakeUpdater {
    pub fn offer(&self, version: &str, url: &str) {
        *self.offer.lock() = Some(Offer {
            version: version.to_owned(),
            url: url.to_owned(),
        });
    }
}

#[async_trait]
impl Updater for FakeUpdater {
    async fn check(&self, channel: Channel) -> Result<Option<Offer>, UpdateError> {
        self.checks.lock().push(channel);
        Ok(self.offer.lock().clone())
    }

    async fn download(&self, offer: &Offer) -> Result<u64, UpdateError> {
        self.downloads.lock().push(offer.version.clone());
        self.download_error.lock().map_or(Ok(1024), Err)
    }

    async fn install(&self, version: &str) -> Result<(), UpdateError> {
        self.installs.lock().push(version.to_owned());
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeUpdateEnv {
    pub server: Mutex<ServerVersion>,
    pub offline: Mutex<bool>,
    pub activity: Mutex<Activity>,
    pub held: Mutex<Vec<bool>>,
}

#[async_trait]
impl UpdateEnv for FakeUpdateEnv {
    async fn server_version(&self) -> Option<ServerVersion> {
        (!*self.offline.lock()).then(|| self.server.lock().clone())
    }

    async fn activity(&self) -> Activity {
        self.activity.lock().clone()
    }

    async fn hold_too_old(&self, held: bool) {
        self.held.lock().push(held);
    }
}
