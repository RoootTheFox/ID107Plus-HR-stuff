#![feature(async_closure)]

mod util;

use btleplug::api::{Central, CentralEvent, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::error::Error;
use std::process::exit;
use std::time::Duration;
use btleplug::api::bleuuid::uuid_from_u16;
use enigo::{Enigo, Key};
use futures::StreamExt;

use tokio::time;
use crate::util::{key_down, key_up};

#[macro_use]
extern crate tracing;

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
    tracing_subscriber::fmt::init();

    info!("Creating manager");
    let manager = Manager::new().await?;

    info!("Searching for adapter");

    let central = get_central(&manager).await;

    info!("Scanning for devices");

    let mut events = central.events().await?;

    central.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                //println!("DeviceDiscovered: {:?}", id);
                let p = central.peripheral(&id).await.unwrap();
                if is_tracker(&p).await {
                    info!("Found device: {:?}", p.properties().await.unwrap().unwrap().local_name);
                    p.connect().await?;
                    info!("Connected to tracker");
                    p.discover_services().await?;
                    let characteristics = p.characteristics();
                    info!("Found {} characteristics", characteristics.len());

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
                    info!("Found request characteristic: {:?}", request_ch);

                    // for receiving data
                    let response_ch = p.characteristics().iter().find(|c| c.uuid == uuid_from_u16(0x0af7)).unwrap().clone();
                    info!("Found response characteristic: {:?}", response_ch);

                    let cmd = vec![0x02, 0xa0];
                    p.write(&request_ch, &cmd, WriteType::WithResponse).await?;

                    let mut notification_stream =
                        p.notifications().await?;

                    tokio::spawn(async move {
                        let mut enigo = Enigo::new();

                        while let Some(notification) = notification_stream.next().await {
                            if notification.uuid == response_ch.uuid {
                                let data = notification.value;
                                if data.len() < 3 {
                                    warn!("Invalid data - [{}] {:?}", notification.uuid, data);
                                    continue;
                                }

                                let n_type = u16::from_le_bytes([data[0], data[1]]);
                                let n_data = &data[2..];

                                match n_type {
                                    0xa002 => {
                                        let hr = n_data[n_data.len() - 1];
                                        //info!("Heart rate: {}", hr);

                                        let cmd_clone = cmd.clone();
                                        let p_clone = p.clone();
                                        let request_ch_clone = request_ch.clone();

                                        tokio::task::spawn(async move {
                                            time::sleep(Duration::from_secs(1)).await;
                                            p_clone.write(&request_ch_clone, &cmd_clone, WriteType::WithResponse).await.expect("Failed to write");
                                        });
                                    },
                                    0x0107 => {
                                        info!("Button pressed");
                                        key_down(&mut enigo, Key::Space);
                                        key_up(&mut enigo, Key::Space);
                                    }
                                    _ => {
                                        warn!("Unknown type {:x} - Data: {:?}", n_type, n_data);
                                    }
                                }
                            } else {
                                warn!("unknown notification: {:?}", notification);
                            }
                        }
                    });
                    info!("Waiting for notifications");
                }
            }
            CentralEvent::DeviceConnected(id) => {
                info!("Connected to device {:?}", id);
            }
            CentralEvent::DeviceDisconnected(id) => {
                println!("Disconnected from {:?}", id);
                exit(0);
            }
            _ => {}
        }
    }

    Ok(())
}
