
extern crate bme680;
extern crate embedded_hal;
extern crate env_logger;

extern crate influent;

#[macro_use]
extern crate log;

extern crate kankyo;



// the library for the embedded linux device
extern crate linux_embedded_hal as lin_hal;



use influent::create_secure_client;
use influent::client::{Client, Credentials};
use influent::measurement::{Measurement, Value};





use lin_hal::Delay;

use bme680::*;
use embedded_hal::blocking::i2c;



use lin_hal::*;
use std::result;
use std::thread;
use std::time::Duration;


use std::env;



/**/

fn main() 
    -> result::Result<(), Error<<lin_hal::I2cdev as i2c::Read>::Error, <lin_hal::I2cdev as i2c::Write>::Error>>
{
    env_logger::init();

    kankyo::load().expect("Loading .env File didn't work");

    // Set up Influx client
    let credentials = Credentials {
        username: &env::var("INFLUX_USER").expect("influx user in .env"),
        password: &env::var("INFLUX_PASSWORD").expect("influx password in .env"),
        database: &env::var("INFLUX_DATABASE").expect("influx database in .env")
    };

    let address = &env::var("INFLUX_ADDRESS").expect("influx address in .env");
    let hosts: Vec<&str> = vec![address];
    let client = create_secure_client(credentials, hosts);

    // use the id to seperate and identify this special device in the databases
    // if you have multiple running
    let id = &env::var("BME680_ID").unwrap_or("Default ID".to_string());

    let i2c = I2cdev::new("/dev/i2c-1").unwrap();

    let mut dev = Bme680::init(i2c, Delay {}, I2CAddress::Secondary)?;

    let settings = SettingsBuilder::new()
        .with_humidity_oversampling(OversamplingSetting::OS2x)
        .with_pressure_oversampling(OversamplingSetting::OS4x)
        .with_temperature_oversampling(OversamplingSetting::OS8x)
        .with_temperature_filter(IIRFilterSize::Size3)
        .with_gas_measurement(Duration::from_millis(1500), 320, 25)
        .with_run_gas(true)
        .build();

    let profile_dur = dev.get_profile_dur(&settings.0)?;
    info!("Profile duration {:?}", profile_dur);
    info!("Setting sensor settings");
    dev.set_sensor_settings(settings)?;
    info!("Setting forced power modes");
    dev.set_sensor_mode(PowerMode::ForcedMode)?;

    let sensor_settings = dev.get_sensor_settings(settings.1);
    info!("Sensor settings: {:?}", sensor_settings);

    let mut warm_up = 0;

    loop {

        // Retrieve sensor data
        dev.set_sensor_mode(PowerMode::ForcedMode)?;
        info!("Retrieving sensor data");
        let (data, state) = dev.get_sensor_data()?;


        info!("Sensor Data {:?}", data);
        info!("Temperature {}°C", data.temperature_celsius());
        info!("Pressure {}hPa", data.pressure_hpa());
        info!("Humidity {}%", data.humidity_percent());
        info!("Gas Resistence {}Ω", data.gas_resistance_ohm());

        // sensor needs a bit time to warm up until it transmits the correct data
        // so we don't transmit the first minute of sensor data
        if warm_up > 12 {
            // Send the Data to the Influx Database
            if state == FieldDataCondition::NewData {
                send_value(&client, "temperature" ,Value::Float(data.temperature_celsius() as f64), id);
                send_value(&client, "pressure" ,Value::Float(data.pressure_hpa() as f64), id);
                send_value(&client, "humidity" ,Value::Float(data.humidity_percent() as f64), id);
                send_value(&client, "gasresistence" , Value::Float(data.gas_resistance_ohm() as f64), id);
            }
        } else {
            warm_up += 1;
        }
        

        //Sleep for 5 seconds
        thread::sleep(Duration::from_millis(5000));
    }
    
}

/// Sends a measured value to the influx database
fn send_value(client:&Client, type_name: &str, value: Value, id: &str) {
    let mut measurement = Measurement::new("sensor");
    measurement.add_field("value", value);
    measurement.add_tag("id", id);
    measurement.add_tag("name", "bme680");
    measurement.add_tag("type", type_name);

    if let Err(e) = client.write_one(measurement, None) {
        info!("Client couldn't connect to InfluxDB Server: {:?}", e);
    }
}


