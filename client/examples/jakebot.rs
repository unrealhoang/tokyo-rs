use common::models::*;
use euclid::Angle;
use std::time::{Duration, Instant};
use tokyo::{self, analyzer::Analyzer, geom::*, Handler};

enum State {
    Spray(usize, Angle<f32>),
    Dodging(usize),
}

#[derive(Default)]
struct Player {
    analyzer: Analyzer,
    state: Option<State>,
}

const SPRAY_ANGLE: f32 = 15.0;

impl Player {
    fn approach_closest(&mut self) -> Option<GameCommand> {
        let me = self.analyzer.own_player();
        if let Some(closest) = self.analyzer.player_closest() {
            if me.distance(closest) < 200.0 {
                None
            } else {
                let angle = me.angle_to(closest);
                if (me.angle - angle).abs() > Angle::degrees(1.0) {
                    Some(GameCommand::Rotate(angle.get()))
                } else {
                    Some(GameCommand::Forward(1.0))
                }
            }
        } else {
            None
        }
    }
}

impl Handler for Player {
    fn tick(&mut self, state: &ClientState) -> Option<GameCommand> {
        self.analyzer.push_state(state, Instant::now());

        let me = self.analyzer.own_player();
        if let Some(State::Dodging(count)) = self.state {
            println!("TRYNA DODGE");
            self.state = if count > 0 { Some(State::Dodging(count - 1)) } else { None };
            Some(GameCommand::Forward(1.0))
        } else if self.analyzer.bullets_colliding(Duration::from_millis(2000)).len() > 0 {
            println!("OK TIME TO DODGE SOME BULLETS");
            self.state = Some(State::Dodging(5));
            Some(GameCommand::Rotate((me.angle + Angle::degrees(90.0)).get()))
        } else if let Some(State::Spray(count, angle)) = self.state {
            if (me.angle - angle).abs() > Angle::degrees(1.0) {
                Some(GameCommand::Rotate(angle.get()))
            } else {
                self.state = if count > 1 {
                    Some(State::Spray(count - 1, angle + Angle::degrees(SPRAY_ANGLE)))
                } else {
                    None
                };
                Some(GameCommand::Fire)
            }
        } else if let Some(closest) = self.analyzer.player_closest() {
            if self.analyzer.own_bullets_count() < 1 {
                let angle = me.angle_to(closest);
                self.state = Some(State::Spray(3, angle - Angle::degrees(SPRAY_ANGLE)));
                Some(GameCommand::Rotate(angle.get()))
            } else {
                self.approach_closest()
            }
        } else {
            self.approach_closest()
        }
    }
}

fn main() {
    println!("starting up...");
    tokyo::run("jakebot", "jakebot", Player::default()).unwrap();
}
