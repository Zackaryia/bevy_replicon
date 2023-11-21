//! A simple demo to showcase how player could send inputs to move the square and server replicates position back.
//! Also demonstrates the single-player and how sever also could be a player.

// Run 
// cargo run --no-default-features --features server
// CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-server-runner RUSTFLAGS=--cfg=web_sys_unstable_apis cargo run --target wasm32-unknown-unknown --no-default-features --features client
use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    path::PathBuf,
};

use bevy::prelude::*;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use bevy_replicon::{
    prelude::*,
    renet::{
        transport::{
            ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport,
            ServerAuthentication, ServerConfig,
        },
        ClientId, ConnectionConfig, ServerEvent,
    },
};

use bevy::prelude::Resource;


#[cfg(feature = "client")]
use wasm_rs_async_executor::single_threaded as executor;
#[cfg(feature = "client")]
use renet_webtransport::prelude::*;
#[cfg(feature = "client")]
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

#[cfg(feature = "server")]
use renet_webtransport_server::*;
#[cfg(feature = "server")]
use bevy_tokio_tasks;


const PORT: u16 = 5001;

fn main() {
    #[cfg(not(feature = "client"))]
    #[cfg(not(feature = "server"))]
    {
        unreachable!("Must enable either client or server")
    }
    
    App::new()
        .add_plugins((DefaultPlugins, ReplicationPlugins, SimpleBoxPlugin))
        .run();
}

struct SimpleBoxPlugin;

impl Plugin for SimpleBoxPlugin {
    fn build(&self, app: &mut App) {
        app.replicate::<PlayerPosition>()
            .replicate::<PlayerColor>()
            .add_client_event::<MoveDirection>(EventType::Ordered)
            .add_systems(
                Startup,
                (Self::cli_system.map(Result::unwrap), Self::init_system),
            )
            .add_systems(
                Update,
                (
                    Self::movement_system.run_if(has_authority()), // Runs only on the server or a single player.
                    Self::server_event_system.run_if(resource_exists::<RenetServer>()), // Runs only on the server.
                    (Self::draw_boxes_system, Self::input_system),
                ),
            );

        #[cfg(feature = "server")]
        {
            app.add_plugins(bevy_tokio_tasks::TokioTasksPlugin::default());
        }
    }
}

#[cfg(feature = "server")]
async fn create_server(server_config: WebTransportConfig) -> WebTransportServer {
    WebTransportServer::new(server_config).unwrap()
}

#[cfg(feature = "client")]
async fn create_client() -> WebTransportClient {
    WebTransportClient::new("https://127.0.0.1:5001", None).await.unwrap()
}

impl SimpleBoxPlugin {
    fn cli_system(
        mut commands: Commands,
        network_channels: Res<NetworkChannels>,
        #[cfg(feature = "server")]
        runtime: ResMut<bevy_tokio_tasks::TokioTasksRuntime>,
    ) -> Result<(), Box<dyn Error>> {
        #[cfg(feature = "server")]
        {
            let server_channels_config = network_channels.get_server_configs();
            let client_channels_config = network_channels.get_client_configs();

            let server = RenetServer::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                ..Default::default()
            });

            // let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let public_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), PORT);
            // let socket = UdpSocket::bind(public_addr)?;
            
            // let server_config = ServerConfig {
            //     current_time,
            //     max_clients: 10,
            //     protocol_id: PROTOCOL_ID,
            //     authentication: ServerAuthentication::Unsecure,
            //     public_addresses: vec![public_addr],
            // };

            let server_config = WebTransportConfig {
                listen: public_addr,
                cert: PathBuf::from("/home/z/Desktop/bevy_replicon/examples/localhost.der"),
                key: PathBuf::from("/home/z/Desktop/bevy_replicon/examples/localhost_key.der"),
                max_clients: 10,
            };

            let transport = runtime.runtime().block_on(create_server(server_config));

            commands.insert_resource(transport);
            commands.insert_resource(server);

            commands.spawn(TextBundle::from_section(
                "Server",
                TextStyle {
                    font_size: 30.0,
                    color: Color::WHITE,
                    ..default()
                },
            ));
            commands.spawn(PlayerBundle::new(SERVER_ID, Vec2::ZERO, Color::GREEN));
        }

        #[cfg(feature = "client")]
        {
            let server_channels_config = network_channels.get_server_configs();
            let client_channels_config = network_channels.get_client_configs();

            let client = RenetClient::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                ..Default::default()
            });

            let client_id = rand::thread_rng().next_u64();
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), PORT);
            let authentication = ClientAuthentication::Unsecure {
                client_id,
                protocol_id: PROTOCOL_ID,
                server_addr,
                user_data: None,
            };

            let connection_config = ConnectionConfig::default();

            let transport = executor::block_on(create_client());
            // Ok(Self {
            //     renet_client: client,
            //     web_transport_client: transport,
            //     duration: 0.0,
            //     messages: Vec::with_capacity(20),
            // })


            commands.insert_resource(client);
            commands.insert_resource(transport);

            commands.spawn(TextBundle::from_section(
                format!("Client: {client_id:?}"),
                TextStyle {
                    font_size: 30.0,
                    color: Color::WHITE,
                    ..default()
                },
            ));
        }

        Ok(())
    }

    fn init_system(mut commands: Commands) {
        commands.spawn(Camera2dBundle::default());
    }

    /// Logs server events and spawns a new player whenever a client connects.
    fn server_event_system(mut commands: Commands, mut server_event: EventReader<ServerEvent>) {
        for event in server_event.read() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    info!("player: {client_id} Connected");
                    // Generate pseudo random color from client id.
                    let r = ((client_id.raw() % 23) as f32) / 23.0;
                    let g = ((client_id.raw() % 27) as f32) / 27.0;
                    let b = ((client_id.raw() % 39) as f32) / 39.0;
                    commands.spawn(PlayerBundle::new(
                        *client_id,
                        Vec2::ZERO,
                        Color::rgb(r, g, b),
                    ));
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    info!("client {client_id} disconnected: {reason}");
                }
            }
        }
    }

    fn draw_boxes_system(mut gizmos: Gizmos, players: Query<(&PlayerPosition, &PlayerColor)>) {
        for (position, color) in &players {
            gizmos.rect(
                Vec3::new(position.x, position.y, 0.0),
                Quat::IDENTITY,
                Vec2::ONE * 50.0,
                color.0,
            );
        }
    }

    /// Reads player inputs and sends [`MoveCommandEvents`]
    fn input_system(mut move_events: EventWriter<MoveDirection>, input: Res<Input<KeyCode>>) {
        let mut direction = Vec2::ZERO;
        if input.pressed(KeyCode::Right) {
            direction.x += 1.0;
        }
        if input.pressed(KeyCode::Left) {
            direction.x -= 1.0;
        }
        if input.pressed(KeyCode::Up) {
            direction.y += 1.0;
        }
        if input.pressed(KeyCode::Down) {
            direction.y -= 1.0;
        }
        if direction != Vec2::ZERO {
            move_events.send(MoveDirection(direction.normalize_or_zero()));
        }
    }

    /// Mutates [`PlayerPosition`] based on [`MoveCommandEvents`].
    ///
    /// Fast-paced games usually you don't want to wait until server send a position back because of the latency.
    /// But this example just demonstrates simple replication concept.
    fn movement_system(
        time: Res<Time>,
        mut move_events: EventReader<FromClient<MoveDirection>>,
        mut players: Query<(&Player, &mut PlayerPosition)>,
    ) {
        const MOVE_SPEED: f32 = 300.0;
        for FromClient { client_id, event } in move_events.read() {
            info!("received event {event:?} from client {client_id}");
            for (player, mut position) in &mut players {
                if *client_id == player.0 {
                    **position += event.0 * time.delta_seconds() * MOVE_SPEED;
                }
            }
        }
    }
}

const PROTOCOL_ID: u64 = 0;

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    position: PlayerPosition,
    color: PlayerColor,
    replication: Replication,
}

impl PlayerBundle {
    fn new(client_id: ClientId, position: Vec2, color: Color) -> Self {
        Self {
            player: Player(client_id),
            position: PlayerPosition(position),
            color: PlayerColor(color),
            replication: Replication,
        }
    }
}

/// Contains the client ID of the player.
#[derive(Component, Serialize, Deserialize)]
struct Player(ClientId);

#[derive(Component, Deserialize, Serialize, Deref, DerefMut)]
struct PlayerPosition(Vec2);

#[derive(Component, Deserialize, Serialize)]
struct PlayerColor(Color);

/// A movement event for the controlled box.
#[derive(Debug, Default, Deserialize, Event, Serialize)]
struct MoveDirection(Vec2);
