use std::time::Instant;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use std::collections::HashSet;
use tokyo::models::{BULLET_RADIUS, BULLET_SPEED, BulletState, DeadPlayer, GameCommand, GameConfig, GameState, PLAYER_BASE_SPEED, PLAYER_RADIUS, PlayerState};

const DEAD_PUNISH: Duration = Duration::from_secs(3);

pub const TICKS_PER_SECOND: f32 = 30.0;
const MAX_CONCURRENT_BULLETS: usize = 4;

// Time until you start accruing points for surviving
const SURVIVAL_TIMEOUT: u64 = 10;

// Interval for accruing points after reaching the threshold
const SURVIVAL_POINT_INTERVAL: u64 = 4;

pub trait Triangle {
    fn x(&self) -> f32;
    fn y(&self) -> f32;
    fn angle(&self) -> f32;
    fn radius(&self) -> f32;

    fn is_colliding(&self, other: &dyn Triangle) -> bool {
        let d_x = other.x() - self.x();
        let d_y = other.y() - self.y();
        let d_r = other.radius() + self.radius();
        let squared_dist = d_x * d_x + d_y * d_y;
        let squared_radii = d_r * d_r;

        squared_dist < squared_radii
    }
}

impl Triangle for PlayerState {
    fn x(&self) -> f32 {
        self.x
    }

    fn y(&self) -> f32 {
        self.y
    }

    fn angle(&self) -> f32 {
        self.angle
    }

    fn radius(&self) -> f32 {
        PLAYER_RADIUS
    }
}

impl Triangle for BulletState {
    fn x(&self) -> f32 {
        self.x
    }

    fn y(&self) -> f32 {
        self.y
    }

    fn angle(&self) -> f32 {
        self.angle
    }

    fn radius(&self) -> f32 {
        BULLET_RADIUS
    }
}

pub struct Game {
    config: GameConfig,
    pub state: GameState,
    rng: rand::rngs::ThreadRng,
    bullet_id_counter: u32,
    survival_times: HashMap<u32, Instant>,
}

impl Game {
    pub fn new(config: GameConfig) -> Self {
        Self {
            state: GameState::new((config.bound_x, config.bound_y)),
            rng: Default::default(),
            bullet_id_counter: 0,
            survival_times: HashMap::new(),
            config,
        }
    }

    pub fn reset(&mut self) {
        let mut new = Game::new(self.config);
        for player in self.state.players.iter() {
            new.add_player(player.id);
        }
        for corpse in self.state.dead.iter() {
            new.add_player(corpse.player.id);
        }
        let _ = std::mem::replace(self, new);
    }

    fn bounds(&self) -> (f32, f32) {
        (self.config.bound_x, self.config.bound_y)
    }

    pub fn add_player(&mut self, player_id: u32) {
        let mut player = PlayerState::new(player_id);
        let bounds = self.bounds();
        player.randomize(&mut self.rng, bounds);
        self.state.players.push(player);
        self.survival_times.insert(player_id, Instant::now() + Duration::from_secs(SURVIVAL_TIMEOUT));
    }

    pub fn player_left(&mut self, player_id: u32) {
        info!("Player {} left!", player_id);

        if let Some(idx) = self.state.players.iter().position(|p| p.id == player_id) {
            self.state.players.remove(idx);
        }
        if let Some(idx) = self.state.dead.iter().position(|p| p.player.id == player_id) {
            self.state.dead.remove(idx);
        }

        self.survival_times.remove(&player_id);
    }

    pub fn handle_cmd(&mut self, player_id: u32, cmd: GameCommand) {
        // info!("Player {} sent command {:#?}", player_id, cmd);

        if let Some(player) = self.state.players.iter_mut().find(|p| p.id == player_id) {
            match cmd {
                GameCommand::Rotate(angle) => {
                    player.angle = angle;
                },
                GameCommand::Throttle(throttle) => {
                    // Bound and re-map throttle inputs.
                    let throttle = throttle.max(0.0).min(1.0);

                    player.throttle = throttle;
                },
                GameCommand::Fire => {
                    let active_bullets = self
                        .state
                        .bullets
                        .iter()
                        .filter(|bullet| bullet.player_id == player.id)
                        .count();

                    if active_bullets < MAX_CONCURRENT_BULLETS {
                        let bullet_id = self.bullet_id_counter;
                        self.bullet_id_counter = self.bullet_id_counter.wrapping_add(1);

                        let distance_from_player: f32 = 5.0;
                        let (bullet_x, bullet_y) = angle_to_vector(player.angle);

                        self.state.bullets.push(BulletState {
                            id: bullet_id,
                            player_id: player.id,
                            angle: player.angle,
                            x: player.x + (bullet_x * distance_from_player),
                            y: player.y + (bullet_y * distance_from_player),
                        });
                    }
                },
            }
        }
    }

    pub fn init(&mut self) {}

    pub fn tick(&mut self, dt: f32) {
        // Revive the dead
        let now = SystemTime::now();
        let revived = self
            .state
            .dead
            .extract_if(|corpse| corpse.respawn <= now)
            .map(|dead| dead.player)
            .map(|player| {
                println!("revived player {}", player.id);
                player
            });

        self.state.players.extend(revived);

        // Advance bullets
        for bullet in &mut self.state.bullets {
            let (vel_x, vel_y) = angle_to_vector(bullet.angle);

            bullet.x += vel_x * BULLET_SPEED * dt;
            bullet.y += vel_y * BULLET_SPEED * dt;
        }

        for player in &mut self.state.players {
            // Move the player
            let (vel_x, vel_y) = angle_to_vector(player.angle);

            player.x += vel_x * PLAYER_BASE_SPEED * player.throttle * dt;
            player.y += vel_y * PLAYER_BASE_SPEED * player.throttle * dt;

            // Keep the players in bounds
            player.x = player.x.max(PLAYER_RADIUS).min(self.config.bound_x - PLAYER_RADIUS);
            player.y = player.y.max(PLAYER_RADIUS).min(self.config.bound_y - PLAYER_RADIUS);
        }

        let bounds = self.bounds();
        let bound_x = bounds.0;
        let bound_y = bounds.1;

        // Remove out-of-bound bullets
        self.state.bullets.retain(|b| {
            b.x > (BULLET_RADIUS)
                && b.x < (bound_x + BULLET_RADIUS)
                && b.y > (BULLET_RADIUS)
                && b.y < (bound_y + BULLET_RADIUS)
        });

        let mut colliding_buf = HashSet::new();
        for bullet in self.state.bullets.iter() {
            for other in self.state.bullets.iter() {
                if bullet.id != other.id && bullet.is_colliding(other) {
                    colliding_buf.insert(bullet.id);
                    colliding_buf.insert(other.id);
                }
            }
        }
        self.state.bullets.retain(|b| { !colliding_buf.contains(&b.id) });

        // count collisions
        let mut colliding_buf = HashSet::new();
        for player in &self.state.players {
            for other in &self.state.players {
                if player.id != other.id && player.is_colliding(other) {
                    colliding_buf.insert(player.id);
                    colliding_buf.insert(other.id);
                }
            }
        }

        for mut player in self.state.players.extract_if(|player| colliding_buf.contains(&player.id)) {
            player.randomize(&mut self.rng, bounds);
            self.state
                .dead
                .push(DeadPlayer { respawn: SystemTime::now() + DEAD_PUNISH, player });
        }

        // count the dead
        let mut hits = vec![];
        let mut used_bullets = vec![];
        let bounds = self.bounds();

        for bullet in &mut self.state.bullets {
            let deceased = self.state.players.extract_if(|player| {
                if player.is_colliding(bullet) && bullet.player_id != player.id {
                    println!(
                        "Player {} killed player {} at ({}, {})",
                        bullet.player_id, player.id, bullet.x, bullet.y
                    );
                    hits.push(bullet.player_id);
                    used_bullets.push(bullet.id);

                    true
                } else {
                    false
                }
            });
            for mut player in deceased {
                // Reset their survival time bonus
                self.survival_times.insert(player.id, Instant::now() + Duration::from_secs(SURVIVAL_TIMEOUT));

                player.randomize(&mut self.rng, bounds);
                self.state
                    .dead
                    .push(DeadPlayer { respawn: SystemTime::now() + DEAD_PUNISH, player });
            }
        }

        // Clear out used bullets
        self.state.bullets.retain(|b| !used_bullets.contains(&b.id));

        // Update the scoreboard
        for player_id in hits {
            *self.state.scoreboard.entry(player_id).or_default() += 1;
        }

        // Reward players for staying alive
        for (player_id, next_reward_time) in &mut self.survival_times {
            if *next_reward_time <= Instant::now() {
                *self.state.scoreboard.entry(*player_id).or_default() += 1;

                *next_reward_time = Instant::now() + Duration::from_secs(SURVIVAL_POINT_INTERVAL);
            }
        }
    }
}

// TODO(jake): rewrite tests.... maybe

fn angle_to_vector(angle: f32) -> (f32, f32) {
    (angle.cos(), angle.sin())
}
