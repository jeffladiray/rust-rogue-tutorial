use tcod::colors::*;
use tcod::console::*;
use tcod::map::{ FovAlgorithm, Map as FovMap };
use std::cmp;
use rand::Rng;

// NOTICE: General window & game settings
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 24;

// NOTICE: Dungeon settings
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = SCREEN_HEIGHT - 5;

const COLOR_DARK_WALL: Color = Color { 
    r: 0,
    g: 0,
    b: 100
};

const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};

const COLOR_LIGHT_WALL: Color = Color {
    r: 130,
    g: 110,
    b: 50,
};

const COLOR_LIGHT_GROUND: Color = Color {
    r: 200,
    g: 180,
    b: 50,
};

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 5;
const MAX_ROOMS: i32 = 10;
const MAX_ROOM_MONSTERS: i32 = 3;

// NOTICE: FOV parameters
const FOV_ALGORITHM: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

// NOTICE: Player is always first game object
const PLAYER: usize = 0;

struct Tcod {
    root: Root,
    con: Offscreen,
    fov: FovMap,
}


#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Debug)]
struct GameObject {
    x: i32,
    y: i32,
    char: char,
    color: Color,
    name: String,
    blocks: bool,
    is_alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
}

impl GameObject {
    pub fn new(x: i32, y: i32, char: char, color: Color, name: &str, blocks: bool) -> Self {
        GameObject {
            x: x,
            y: y,
            char: char,
            color: color,
            name: name.into(),
            blocks: blocks,
            is_alive: false,
            fighter: None,
            ai: None,
        }
    }

    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &GameObject) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.is_alive = false;
                fighter.on_death.callback(self);
            }
        }
    }

    pub fn attack(&mut self, target: &mut GameObject) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            println!(
                "{} attacks {} for {} hp.",
                self.name, target.name, damage
            );
            target.take_damage(damage);
        } else {
            println!(
                "{} attacks {}, but it has no effect!",
                self.name, target.name
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true,
        }
    }
}

type Map = Vec<Vec<Tile>>;

struct Game {
    map: Map,
}

#[derive(Clone, Copy, Debug)]
struct Rectangle {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rectangle {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rectangle {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h,
        }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;

        (center_x, center_y)
    }

    pub fn is_intersecting(&self, other: &Rectangle) -> bool {
        (self.x1 <= other.x2)
            && (self.x2 >= other.x1)
            && (self.y1 <= other.y2)
            && (self.y2 >= other.y1) 
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
} 

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, game_object: &mut GameObject) {
        use DeathCallback::*;
        let callback: fn(&mut GameObject) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(game_object);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Ai {
    Basic,
}

fn make_room(room: Rectangle, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn make_horizontal_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_vertical_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(game_objects: &mut Vec<GameObject>) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rectangle::new(x, y, w, h);
        let failed = rooms.iter().any(|other_room| new_room.is_intersecting(other_room));
        if !failed {
            make_room(new_room, &mut map);
            place_objects(new_room, &map, game_objects);

            let (new_x, new_y) = new_room.center();
            if rooms.is_empty() {
                game_objects[PLAYER].set_position(new_x, new_y);
            } else {
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();

                if rand::random() {
                    make_horizontal_tunnel(prev_x, new_x, prev_y, &mut map);
                    make_vertical_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    make_vertical_tunnel(prev_y, new_y, prev_x, &mut map);
                    make_horizontal_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }
            rooms.push(new_room);
        }
    }
    map
}

fn place_objects(room: Rectangle, map: &Map, game_objects: &mut Vec<GameObject>) {
    let monster_count = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);
    for _ in 0..monster_count {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);
        if !is_blocked(x, y, map, game_objects) {
            let mut monster = if rand::random::<f32>() < 0.8 {
                let mut orc = GameObject::new(x, y, 'o', DESATURATED_GREEN, "orc", true);
                orc.fighter = Some(Fighter {
                    max_hp: 10,
                    hp: 10,
                    defense: 0,
                    power: 3,
                    on_death: DeathCallback::Monster,
                });

                orc
            } else {
                let mut troll = GameObject::new(x, y, 't', DARKER_GREEN, "troll", true);
                troll.fighter = Some(Fighter {
                    max_hp: 16,
                    hp: 16,
                    defense: 1,
                    power: 4,
                    on_death: DeathCallback::Monster,
                });

                troll
            };
            monster.is_alive = true;
            monster.ai = Some(Ai::Basic);

            game_objects.push(monster);
        }
    }
}

fn is_blocked(x: i32, y: i32, map: &Map, game_objects: &[GameObject]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    game_objects
        .iter()
        .any(|game_object| game_object.blocks && game_object.position() == (x, y))
}

fn move_game_object_by(id: usize, dx: i32, dy: i32, map: &Map, game_objects: &mut [GameObject]) {
    let (x, y) = game_objects[id].position();
    if !is_blocked(x + dx, y + dy, map, game_objects) {
        game_objects[id].set_position(x + dx, y + dy);
    }
}

fn move_game_object_toward(id: usize, target_x: i32, target_y: i32, map: &Map, game_objects: &mut [GameObject]) {
    let dx = target_x - game_objects[id].x;
    let dy = target_y - game_objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_game_object_by(id, dx, dy, map, game_objects);
}

fn render_all(tcod: &mut Tcod, game: &mut Game, game_objects: &Vec<GameObject>, fov_need_recompute: bool) {
    if fov_need_recompute {
        let player = &game_objects[PLAYER];
        tcod.fov.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGORITHM);
    }

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
            let is_wall = game.map[x as usize][y as usize].block_sight;
            let color = match (visible, is_wall) {
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                (true, false) => COLOR_LIGHT_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
            };

            let explored = &mut game.map[x as usize][y as usize].explored;
            if visible {
                // since it's visible, explore it
                *explored = true;
            }
            if *explored {
                // show explored tiles only (any visible tile is explored already)
                tcod.con
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    let mut to_draw: Vec<_> = game_objects
        .iter()
        .filter(|go| tcod.fov.is_in_fov(go.x, go.y))
        .collect();
    to_draw.sort_by(|o1, o2| o1.blocks.cmp(&o2.blocks));

    for game_object in &to_draw {
        game_object.draw(&mut tcod.con);
    }

    tcod.root.set_default_foreground(WHITE);
    if let Some(fighter) = game_objects[PLAYER].fighter {
        tcod.root.print_ex(
            1,
            SCREEN_HEIGHT - 2,
            BackgroundFlag::None,
            TextAlignment::Left,
            format!("HP: {}/{} ", fighter.hp, fighter.max_hp),
        );
    }

    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );
}

fn handle_keys(tcod: &mut Tcod, game: &Game, game_objects: &mut Vec<GameObject>) -> PlayerAction {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let key = tcod.root.wait_for_keypress(true);
    let player_alive = game_objects[PLAYER].is_alive;

    match (key, key.text(), player_alive) {
        (Key { code: Up, .. }, _, true) => {
            player_move_or_attack(0, -1, game, game_objects);
            TookTurn
        }
        (Key { code: Down, .. }, _, true) => {
            player_move_or_attack(0, 1, game, game_objects);
            TookTurn
        }
        (Key { code: Left, .. }, _, true) => {
            player_move_or_attack(-1, 0, game, game_objects);
            TookTurn
        }
        (Key { code: Right, .. }, _, true) => {
            player_move_or_attack(1, 0, game, game_objects);
            TookTurn
        }
        (
            Key {
                code: Enter,
                alt: true,
                ..
            },
            _,
            _,
        ) => {
            let is_fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!is_fullscreen);
            DidntTakeTurn
        }
        (Key { code: Escape, .. }, _, _) => Exit,
        _ => DidntTakeTurn,
    }
}

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &Game, game_objects: &mut Vec<GameObject>) {
    let (monster_x, monster_y) = game_objects[monster_id].position();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if game_objects[monster_id].distance_to(&game_objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = game_objects[PLAYER].position();
            move_game_object_toward(monster_id, player_x, player_y, &game.map, game_objects);
        } else if game_objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, game_objects);
            monster.attack(player);
        }
    }
}

fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}


fn player_move_or_attack(dx: i32, dy: i32, game: &Game, game_objects: &mut [GameObject]) {
    let x = game_objects[PLAYER].x + dx;
    let y = game_objects[PLAYER].y + dy;

    let target_id = game_objects
        .iter()
        .position(|game_object| game_object.fighter.is_some() && game_object.position() == (x, y));

    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, game_objects);
            player.attack(target);
        }
        None => {
            move_game_object_by(PLAYER, dx, dy, &game.map, game_objects);
        }
    }
}

fn player_death(player: &mut GameObject) {
    println!("You died!");

    player.char = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut GameObject) {
    println!("{} is dead!", monster.name);
    monster.char = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("assets/arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust-rogue")
        .init();

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
    };

    let mut player = GameObject::new(25, 23, '@', WHITE, "player", true);
    player.is_alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player,
    });
    let mut game_objects = vec![player];

    let mut game = Game { map: make_map(&mut game_objects) };

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !game.map[x as usize][y as usize].block_sight,
                !game.map[x as usize][y as usize].blocked,
            );
        }
    }

    let mut previous_player_position = (-1, -1);

    while !tcod.root.window_closed() {
        tcod.con.clear();

        let fov_need_recompute = previous_player_position != game_objects[PLAYER].position();
        render_all(&mut tcod, &mut game, &game_objects, fov_need_recompute);

        tcod.root.flush();

        let player = &mut game_objects[PLAYER];
        previous_player_position = (player.x, player.y);
        let player_action = handle_keys(&mut tcod, &game, &mut game_objects);
        if player_action == PlayerAction::Exit {
            break;
        }

        if game_objects[PLAYER].is_alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..game_objects.len() {
                if game_objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, &game, &mut game_objects);
                }
            }
        }
    }
}
