use tokio_util::sync::CancellationToken;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementDataSection, BluetoothLEAdvertisementPublisher,
};
use windows::Storage::Streams::DataWriter;

const SERVICE_DATA: &[u8] = &[
    252, 18, 142, 1, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 191, 45, 91, 160, 225, 216, 117, 36, 202, 0,
];

/// BLE AD type for 16-bit UUID service data
const AD_TYPE_SERVICE_DATA_16BIT: u8 = 0x16;

const INNER_NAME: &str = "BleAdvertiser";

#[derive(Debug, Clone)]
pub struct BleAdvertiser {
    publisher: BluetoothLEAdvertisementPublisher,
}

impl BleAdvertiser {
    pub async fn new() -> Result<Self, anyhow::Error> {
        let publisher = BluetoothLEAdvertisementPublisher::new()?;

        let writer = DataWriter::new()?;
        // AD type 0x16: 16-bit UUID (little-endian) followed by service data
        // 0xFE2C -> [0x2C, 0xFE] in little-endian
        writer.WriteByte(0x2C)?;
        writer.WriteByte(0xFE)?;
        for &b in SERVICE_DATA {
            writer.WriteByte(b)?;
        }
        let buffer = writer.DetachBuffer()?;

        let section = BluetoothLEAdvertisementDataSection::new()?;
        section.SetDataType(AD_TYPE_SERVICE_DATA_16BIT)?;
        section.SetData(&buffer)?;

        publisher.Advertisement()?.DataSections()?.Append(&section)?;

        Ok(Self { publisher })
    }

    pub async fn run(&self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!("{INNER_NAME}: starting BLE advertisement");
        self.publisher.Start()?;
        info!("{INNER_NAME}: advertising started");

        ctk.cancelled().await;

        info!("{INNER_NAME}: tracker cancelled, stopping");
        self.publisher.Stop()?;

        Ok(())
    }
}
