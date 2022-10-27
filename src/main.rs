#![feature(async_closure)]

use btleplug::api::{Central, CentralEvent, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::error::Error;
use std::time::Duration;
use btleplug::api::bleuuid::{BleUuid, uuid_from_u16};
use btleplug::Error::Uuid;
use futures::StreamExt;

use tokio::{task, time};

async fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().await.unwrap();
    adapters.into_iter().nth(0).unwrap()
}

async fn is_tracker(p: &Peripheral) -> bool {
    if p.properties()
        .await
        .unwrap()
        .unwrap()
        .local_name
        .iter()
        .any(|name| name.contains("ID107Plus HR"))
    {
        return true;
    }
    false
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    println!("Creating manager");
    let manager = Manager::new().await?;

    println!("Searching for adapter");

    let central = get_central(&manager).await;

    println!("Scanning for devices");

    let mut events = central.events().await?;

    central.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                //println!("DeviceDiscovered: {:?}", id);
                let p = central.peripheral(&id).await.unwrap();
                if is_tracker(&p).await {
                    println!("Found tracker: {:?}", p.properties().await.unwrap().unwrap().local_name);
                    p.connect().await?;
                    println!("Connected to tracker");
                    p.discover_services().await?;
                    println!("Discovered services");
                    let characteristics = p.characteristics();
                    println!("Found {} characteristics", characteristics.len());

                    let services = p.services();
                    for service in services {
                        //println!("Service: {:?}", service);
                        let characteristics = service.characteristics;
                        for characteristic in characteristics {
                            //println!("Characteristic: {:?}", characteristic);
                            if characteristic.properties.contains(CharPropFlags::NOTIFY) {
                                println!("Subscribing to characteristic {:?}", characteristic.uuid);
                                p.subscribe(&characteristic).await?;
                            }
                        }
                    }

                    // for sending commands
                    let request_ch = p.characteristics().iter().find(|c| c.uuid == uuid_from_u16(0x0af6)).unwrap().clone();

                    // for receiving data
                    let response_ch = p.characteristics().iter().find(|c| c.uuid == uuid_from_u16(0x0af7)).unwrap().clone();

                    let cmd = vec![0x02, 0xa0];
                    p.write(&request_ch, &cmd, WriteType::WithResponse).await?;

                    let mut notification_stream =
                        p.notifications().await?;

                    let request_ch_clone = request_ch.clone();

                    tokio::spawn(async move {
                        while let Some(notification) = notification_stream.next().await {
                            if notification.uuid == response_ch.uuid {
                                let data = notification.value;
                                if data.len() < 3 {
                                    println!("Invalid data - [{}] {:?}", notification.uuid, data);
                                    continue;
                                }

                                let n_type = u16::from_le_bytes([data[0], data[1]]);
                                let n_data = &data[2..];

                                match n_type {
                                    0xa002 => {
                                        let hr = n_data[n_data.len() - 1];
                                        println!("Heart rate: {}", hr);

                                        time::sleep(Duration::from_secs(1)).await;
                                        p.write(&request_ch_clone, &cmd, WriteType::WithResponse).await.expect("Unable to write");
                                    },
                                    0x0107 => {
                                        println!("Button pressed");
                                    }
                                    _ => {
                                        println!("Unknown type {:x} - Data: {:?}", n_type, n_data);
                                    }
                                }
                            } else {
                                println!("unknown notification: {:?}", notification);
                            }
                        }
                    });
                    println!("Waiting for notifications");
                }
            }
            CentralEvent::DeviceConnected(id) => {
                println!("DeviceConnected: {:?}", id);
            }
            CentralEvent::DeviceDisconnected(id) => {
                println!("DeviceDisconnected: {:?}", id);
            }
            _ => {}
        }
    }

    time::sleep(Duration::from_secs(2)).await;

    /*

    // find the characteristic we want
    let chars = tracker.characteristics();
    let cmd_char = chars
        .iter()
        .find(|c| c.uuid == LIGHT_CHARACTERISTIC_UUID)
        .expect("Unable to find characterics");

    // dance party
    let mut rng = thread_rng();
    for _ in 0..20 {
        let color_cmd = vec![0x56, rng.gen(), rng.gen(), rng.gen(), 0x00, 0xF0, 0xAA];
        tracker
            .write(&cmd_char, &color_cmd, WriteType::WithoutResponse)
            .await?;
        time::sleep(Duration::from_millis(200)).await;
    }
    */
    Ok(())
}
