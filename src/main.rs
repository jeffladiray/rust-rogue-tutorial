use tcod::colors::*;
use tcod::console::*;
use tcod::map::{ FovAlgorithm, Map as FovMap };
use tcod::input::{ self, Event, Key, Mouse };
use std::cmp;
use rand::Rng;

// NOTICE: General window & game settings
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 24;

const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

// NOTICE: Dungeon settings
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

const COLOR_DARK_WALL: Color = Color { 
    r: 111,
    g: 103,
    b: 118,
};

const COLOR_DARK_GROUND: Color = Color {
    r: 154,
    g: 154,
    b: 151,
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

// NOTICE: Inventory constants 
const MAX_ROOM_ITEMS: i32 = 3;
const INVENTORY_WIDTH: i32 = 50;
const HEAL_AMOUNT: i32 = 4;
const LIGHTNING_RANGE: i32 = 5;
const LIGHTNING_DAMAGE: i32 = 40;
const CONFUSION_RANGE: i32 = 5;
const CONFUSE_TURN_COUNT: i32 = 10;

// NOTICE: FOV parameters
const FOV_ALGORITHM: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

// NOTICE: Player is always first game object
const PLAYER: usize = 0;

// NOTICE: Panel messages bar
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
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
    item: Option<Item>,
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
            item: None,
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

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.is_alive = false;
                fighter.on_death.callback(self, game);
            }
        }
    }

    pub fn attack(&mut self, target: &mut GameObject, game: &mut Game) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            game.messages.add(
                format!(
                    "{} attacks {} for {} hp.",
                    self.name, target.name, damage
                ),
                WHITE,
            );
            target.take_damage(damage, game);
        } else {
            game.messages.add(
                format!(
                    "{} attacks {}, but it has no effect!",
                    self.name, target.name
                ),
                WHITE,
            );
        }
    }

    pub fn heal(&mut self, amount: i32) {
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > fighter.max_hp {
                fighter.hp = fighter.max_hp;
            }
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
    messages: Messages,
    inventory: Vec<GameObject>,
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
    fn callback(self, game_object: &mut GameObject, game: &mut Game) {
        use DeathCallback::*;
        let callback: fn(&mut GameObject, &mut Game) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(game_object, game);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Ai {
    Basic,
    Confused {
        previous_ai: Box<Ai>,
        num_turns: i32,
    }
}

struct Messages {
    messages: Vec<(String, Color)>,
}

impl Messages {
    pub fn new () -> Self {
        Self { messages: vec![] }
    }

    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Heal,
    ScrollOfLightning,
    ScrollOfConfusion,
}

enum UseResult {
    UsedUp,
    Cancelled,
}

fn pick_item_up(object_id: usize, game: &mut Game, game_objects: &mut Vec<GameObject>) {
    if game.inventory.len() >= 9 {
        game.messages.add(
            format!(
                "Cannot pickup {}, inventory is full!",
                game_objects[object_id].name,
            ),
            RED,
        );
    } else {
        let item = game_objects.swap_remove(object_id);
        game.messages.add(
            format!(
                "You picked up {}",
                item.name
            ),
            GREEN,
        );
        game.inventory.push(item);
    }
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
            place_game_objects(new_room, &map, game_objects);

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

fn place_game_objects(room: Rectangle, map: &Map, game_objects: &mut Vec<GameObject>) {
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

    let item_count = rand::thread_rng().gen_range(0, MAX_ROOM_ITEMS + 1);

    for _ in 0..item_count {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if !is_blocked(x, y, map, game_objects) {
            let dice = rand::random::<f32>();
            let item = if dice < 0.7 {
                let mut game_object = GameObject::new(
                    x,
                    y,
                    '!',
                    VIOLET,
                    "healing potion",
                    false
                );
                game_object.item = Some(Item::Heal);
                game_object
            } else if dice < 0.8 {
                let mut game_object = GameObject::new(
                    x,
                    y,
                    '~',
                    LIGHT_YELLOW,
                    "scroll of lightning bolt",
                    false
                );
                game_object.item = Some(Item::ScrollOfLightning);
                game_object
            } else {
                let mut game_object = GameObject::new(x, y, 'c', LIGHT_YELLOW, "scroll of confusion", false);
                game_object.item = Some(Item::ScrollOfConfusion);
                game_object
            };

            game_objects.push(item);
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

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum),
    );
}

fn render_all(tcod: &mut Tcod, game: &mut Game, game_objects: &Vec<GameObject>, fov_need_recompute: bool) {

    match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
        Some((_, Event::Mouse(m))) => tcod.mouse = m,
        Some((_, Event::Key(k))) => tcod.key = k,
        _ => tcod.key = Default::default(),
    }

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
                *explored = true;
            }
            if *explored {
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

    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
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

    let player_hp = game_objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let player_max_hp = game_objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        player_hp,
        player_max_hp,
        LIGHT_RED,
        DARKER_RED,
    );

    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, game_objects, &tcod.fov),
    );


    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, PANEL_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );
}

fn get_names_under_mouse(mouse: Mouse, game_objects: &Vec<GameObject>, fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    let names = game_objects
        .iter()
        .filter(|game_object| game_object.position() == (x, y) && fov_map.is_in_fov(game_object.x, game_object.y))
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    names.join(", ")
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) -> PlayerAction {
    use PlayerAction::*;
    use tcod::input::KeyCode::*;

    let player_alive = game_objects[PLAYER].is_alive;

    match (tcod.key, tcod.key.text(), player_alive) {
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
        (Key { code: Text, .. }, "g", true) => {
            let item_id = game_objects
                .iter()
                .position(|game_object| game_object.position() == game_objects[PLAYER].position() && game_object.item.is_some());
            if let Some(item_id) = item_id {
                pick_item_up(item_id, game, game_objects);
            }
            DidntTakeTurn
        }
        (Key { code: Text, ..}, "i", true) => {
            let inventory_index = inventory_menu(
                &mut game.inventory,
                "Press the key next to an item to use it, or any other to cancel.\n",
                &mut tcod.root,
            );
            if let Some(inventory_index) = inventory_index {
                use_item(inventory_index, tcod, game, game_objects);
            }
            DidntTakeTurn
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

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) {
    use Ai::*;
    if let Some(ai) = game_objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, tcod, game, game_objects),
            Confused {
                previous_ai,
                num_turns,
            } => ai_confused(monster_id, tcod, game, game_objects, previous_ai, num_turns),
        };
        game_objects[monster_id].ai = Some(new_ai);
    }
}

fn ai_basic(monster_id: usize, tcod: &Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) -> Ai {
    let (monster_x, monster_y) = game_objects[monster_id].position();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if game_objects[monster_id].distance_to(&game_objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = game_objects[PLAYER].position();
            move_game_object_toward(monster_id, player_x, player_y, &game.map, game_objects);
        } else if game_objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, game_objects);
            monster.attack(player, game);
        }
    }
    Ai::Basic
}

fn ai_confused(monster_id: usize, _tcod: &Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>, previous_ai: Box<Ai>, num_turns: i32) -> Ai {
    if num_turns >= 0 {
        move_game_object_by(
            monster_id,
            rand::thread_rng().gen_range(-1, 2),
            rand::thread_rng().gen_range(-1, 2),
            &game.map,
            game_objects,
        );
        Ai::Confused {
            previous_ai: previous_ai,
            num_turns: num_turns - 1,
        }
    } else {
        game.messages.add(
            format!(
                "{} is no longer confused",
                game_objects[monster_id].name,
            ),
            WHITE,
        );
        *previous_ai
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


fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, game_objects: &mut [GameObject]) {
    let x = game_objects[PLAYER].x + dx;
    let y = game_objects[PLAYER].y + dy;

    let target_id = game_objects
        .iter()
        .position(|game_object| game_object.fighter.is_some() && game_object.position() == (x, y));

    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, game_objects);
            player.attack(target, game);
        }
        None => {
            move_game_object_by(PLAYER, dx, dy, &game.map, game_objects);
        }
    }
}

fn player_death(player: &mut GameObject, game: &mut Game) {
    game.messages.add(
        "You died!",
        RED,
    );

    player.char = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut GameObject, game: &mut Game) {
    game.messages.add(
        format!(
            "{} is dead !",
            monster.name,
        ),
        ORANGE,
    );
    monster.char = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(
        options.len() <= 9,
        "Cannot have a menu with more than 9 options."
    );

    let header_height = root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header);
    let height = options.len() as i32 + header_height;

    let mut window = Offscreen::new(width, height);
    window.set_default_foreground(WHITE);
    window.print_rect_ex(
        0,
        0,
        width,
        height,
        BackgroundFlag::None,
        TextAlignment::Left,
        header,
    );

    for (index, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(
            0,
            header_height + index as i32,
            BackgroundFlag::None,
            TextAlignment::Left,
            text,
        );
    }

    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    blit(&window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    root.flush();
    let key = root.wait_for_keypress(true);
    if key.printable.is_digit(10) {
        let index = (key.printable.to_digit(10).unwrap() - 1) as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn use_item(inventory_id: usize, tcod: &mut Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) {
    use Item::*;

    if let Some(item) = game.inventory[inventory_id].item {
        let on_use = match item {
            Heal => cast_heal,
            ScrollOfLightning => cast_lightning,
            ScrollOfConfusion => cast_confusion,
        };
        match on_use(inventory_id, tcod, game, game_objects) {
            UseResult::UsedUp => {
                game.inventory.remove(inventory_id);
            }
            UseResult::Cancelled => {
                game.messages.add("Cancelled", WHITE);
            }
        }
    } else {
        game.messages.add(
            format!("The {} cannot be used", game.inventory[inventory_id].name),
            WHITE,
        )
    }
}

fn cast_confusion(_inventory_id: usize, tcod: &mut Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) -> UseResult {
    let monster_id = closest_monster(tcod, game_objects, CONFUSION_RANGE);
    if let Some(monster_id) = monster_id {
        let old_ai = game_objects[monster_id].ai.take().unwrap_or(Ai::Basic);
        game_objects[monster_id].ai = Some(Ai::Confused {
            previous_ai: Box::new(old_ai),
            num_turns: CONFUSE_TURN_COUNT,
        });
        game.messages.add(
            format!(
                "{} is confused !",
                game_objects[monster_id].name,
            ),
            WHITE,
        );
        UseResult::UsedUp 
    } else {
        game.messages.add(
            "There is no enemy to strike.",
            RED,
        );
        UseResult::Cancelled
    }
}

fn cast_lightning(_inventory_id: usize, tcod: &mut Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) -> UseResult {
    let monster_id = closest_monster(tcod, game_objects, LIGHTNING_RANGE);
    if let Some(monster_id) = monster_id {
        game.messages.add(
            format!(
                "A lightning bolt strikes the {} and damaged it {} hit points!",
                game_objects[monster_id].name, LIGHTNING_DAMAGE
            ),
            LIGHT_BLUE,
        );
        game_objects[monster_id].take_damage(LIGHTNING_DAMAGE, game);
        UseResult::UsedUp
    } else {
        game.messages.add(
            "There is no enemy to strike.",
            RED,
        );
        UseResult::Cancelled
    }
}

fn cast_heal(_inventory_id: usize, _tcod: &mut Tcod, game: &mut Game, game_objects: &mut Vec<GameObject>) -> UseResult {
    if let Some(fighter) = game_objects[PLAYER].fighter {
        if fighter.hp == fighter.max_hp {
            game.messages.add(
                "You are already at full health.",
                RED,
            );

            return UseResult::Cancelled;
        } else {
            game.messages.add(
                "Your wounds start to feel better!",
                LIGHT_VIOLET
            );
            game_objects[PLAYER].heal(HEAL_AMOUNT);
            return UseResult::UsedUp;
        }
    }
    UseResult::Cancelled
}

fn closest_monster(tcod: &Tcod, game_objects: &Vec<GameObject>, max_range: i32) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_distance = (max_range + 1) as f32;
    for (id, game_object) in game_objects.iter().enumerate() {
        if id != PLAYER
            && game_object.fighter.is_some()
            && game_object.ai.is_some()
            && tcod.fov.is_in_fov(game_object.x, game_object.y)
        {
            let distance = game_objects[PLAYER].distance_to(game_object);
            if distance < closest_distance {
                closest_enemy = Some(id);
                closest_distance = distance;
            }
        }
    }
    closest_enemy
}

fn inventory_menu(inventory: &[GameObject], header: &str, root: &mut Root) -> Option<usize> {
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory.iter().map(|item| item.name.clone()).collect()
    };

    let inventory_index = menu(header, &options, INVENTORY_WIDTH, root);

    if inventory.len() > 0 {
        inventory_index
    } else {
        None
    }
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
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
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

    let mut game = Game {
        map: make_map(&mut game_objects),
        messages: Messages::new(),
        inventory: vec![],
    };

    game.messages.add(
        "Welcome adventurer! Prepare to perish in the tomb of the Ancient King !",
        RED,
    );

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
        let player_action = handle_keys(&mut tcod, &mut game, &mut game_objects);
        if player_action == PlayerAction::Exit {
            break;
        }

        if game_objects[PLAYER].is_alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..game_objects.len() {
                if game_objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, &mut game, &mut game_objects);
                }
            }
        }

        tcod.panel.set_default_background(BLACK);
        tcod.panel.clear();
    }
}
