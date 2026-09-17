use crate::engine::ctx::{Cause, Ctx, Event, Location};
pub use crate::state::NameKind;
use crate::state::{Ask, ChainItem, TargetRef};
use agni_plugin_sdk::table::{CardInfo, Snapshot};

pub mod abandon;
pub mod abandoned_hall;
pub mod acceleration_gate;
pub mod acceptable_losses;
pub mod adaptatron;
pub mod affectionate_poro;
pub mod against_the_odds;
pub mod ahri_alluring;
pub mod ahri_inquisitive;
pub mod ahri_nine_tailed_fox;
pub mod akali_deadly_weapon;
pub mod akali_rogue_assassin;
pub mod akali_silent;
pub mod akshan_mischievous;
pub mod albus_ferros;
pub mod allay_eager_admirer;
pub mod alpha_strike;
pub mod alpha_wildclaw;
pub mod altar_of_blood;
pub mod altar_of_memories;
pub mod altar_to_unity;
pub mod amateur_recital;
pub mod ambessa_matriarch_of_war;
pub mod ambessa_respected_and_feared;
pub mod ambessa_the_wolf;
pub mod ancient_henge;
pub mod ancient_warmonger;
pub mod angle_shot;
pub mod angler_beast;
pub mod anivia_primal;
pub mod aphelios_exalted;
pub mod applied_researchers;
pub mod apprentice_mage;
pub mod apprentice_smith;
pub mod arachnoid_horror;
pub mod arcane_shift;
pub mod arena_bar;
pub mod arena_kingpin;
pub mod arise;
pub mod armed_assailant;
pub mod ashe_focused;
pub mod aspirants_climb;
pub mod aspiring_engineer;
pub mod assembly_rig;
pub mod astral_heron;
pub mod atakhan;
pub mod aurok_general;
pub mod ava_achiever;
pub mod azir_ascendant;
pub mod azir_emperor_of_the_sands;
pub mod azir_sovereign;
pub mod b_f_sword;
pub mod baccai_reaper;
pub mod baccai_sandspinner;
pub mod baccai_witherclaw;
pub mod back_alley_bar;
pub mod back_off;
pub mod back_to_back;
pub mod baited_hook;
pub mod bandle_soldier;
pub mod bandle_tree;
pub mod bard_mercurial;
pub mod baron_nashor;
pub mod battering_ram;
pub mod beast_below;
pub mod bellows_breath;
pub mod bewitching_spirit;
pub mod bilgewater_bully;
pub mod black_flame_altar;
pub mod black_market_broker;
pub mod black_rose_dignitary;
pub mod blade_of_the_ruined_king;
pub mod blade_twirler;
pub mod blast_cone;
pub mod blast_corps_cadet;
pub mod blastcone_fae;
pub mod blazing_scorcher;
pub mod blighted_battleaxe;
pub mod blind_fury;
pub mod blitzcrank_impassive;
pub mod block;
pub mod blood_money;
pub mod blood_rose;
pub mod blood_rush;
pub mod blue_sentinel;
pub mod bonds_of_strength;
pub mod bone_skewer;
pub mod boneshiver;
pub mod boots_of_swiftness;
pub mod bottled_constellation;
pub mod brazen_buccaneer;
pub mod breakneck_mech;
pub mod brittle_steel;
pub mod brutal_hunter;
pub mod brutalizer;
pub mod brynhir_thundersong;
pub mod bubble_bot;
pub mod buhru_captain;
pub mod bullet_time;
pub mod bushwhack;
pub mod caitlyn_patrolling;
pub mod call_to_battle;
pub mod call_to_glory;
pub mod called_shot;
pub mod cannon_barrage;
pub mod captain_farron;
pub mod card_sharp;
pub mod carnivorous_snapvine;
pub mod carrion_dredger;
pub mod cataclysmic_duel;
pub mod catalyst_of_aeons;
pub mod cemetery_attendant;
pub mod chakram_dancer;
pub mod challenge;
pub mod charm;
pub mod chemtech_cask;
pub mod chemtech_enforcer;
pub mod cithria_of_cloudfield;
pub mod clairvoyance;
pub mod clash_of_giants;
pub mod cleave;
pub mod clockwork_keeper;
pub mod cloth_armor;
pub mod cloud_drake;
pub mod combat_chef;
pub mod combat_experience;
pub mod commander_ledros;
pub mod concentrate;
pub mod confront;
pub mod conscription;
pub mod consult_the_past;
pub mod consuming_curse;
pub mod convergent_mutation;
pub mod corina_veraza;
pub mod corrupt_enforcer;
pub mod corrupted_dragon;
pub mod counter_strike;
pub mod covert_informant;
pub mod crackshot_corsair;
pub mod crescent_guardian;
pub mod crescent_strike;
pub mod crimson_pigeons;
pub mod crowd_favorite;
pub mod cruel_patron;
pub mod crumbling_sands;
pub mod cull;
pub mod cull_the_weak;
pub mod cursed_sarcophagus;
pub mod curtain_call;
pub mod daisy;
pub mod dame_the_despoiler;
pub mod dancing_grenade;
pub mod danger_zone;
pub mod dangerous_duo;
pub mod daring_poro;
pub mod darius_executioner;
pub mod darius_hand_of_noxus;
pub mod darius_trifarian;
pub mod dauntless_vanguard;
pub mod dazzling_aurora;
pub mod deadbloom_predator;
pub mod deadly_flourish;
pub mod death_from_below;
pub mod death_mark;
pub mod deathgrip;
pub mod decree_of_discord;
pub mod decree_of_focus;
pub mod decree_of_insight;
pub mod decree_of_rage;
pub mod decree_of_strength;
pub mod decree_of_unity;
pub mod defiant_dance;
pub mod defy;
pub mod demacian_diplomat;
pub mod deserts_call;
pub mod determined_sentry;
pub mod detonate;
pub mod diana_lunari;
pub mod diana_no_longer_human;
pub mod diana_scorn_of_the_moon;
pub mod direwing;
pub mod disarming_rake;
pub mod disciple_of_shen;
pub mod discipline;
pub mod disintegrate;
pub mod disposal_order;
pub mod divine_judgment;
pub mod divining_shells;
pub mod dominus;
pub mod dorans_blade;
pub mod dorans_ring;
pub mod dorans_shield;
pub mod double_trouble;
pub mod downstage_dramatics;
pub mod downwell;
pub mod dr_mundo_expert;
pub mod drag_under;
pub mod dragon_form;
pub mod dragon_roost;
pub mod dragons_rage;
pub mod dragonsoul_sage;
pub mod dramatic_visionary;
pub mod draven_audacious;
pub mod draven_glorious_executioner;
pub mod draven_showboat;
pub mod draven_vanquisher;
pub mod dredge_up;
pub mod dropboarder;
pub mod dune_drake;
pub mod dune_surfer;
pub mod dunebreaker;
pub mod dusk_rose_lab;
pub mod eager_apprentice;
pub mod eager_drakehound;
pub mod eclipse;
pub mod eclipse_dragon;
pub mod eclipse_herald;
pub mod edge_of_night;
pub mod ekko_recurrent;
pub mod elder_dragon;
pub mod ember_monk;
pub mod eminent_benefactor;
pub mod emperors_dais;
pub mod emperors_divide;
pub mod en_garde;
pub mod endless_riches;
pub mod energy_conduit;
pub mod enthralling_protector;
pub mod enthusiastic_promoter;
pub mod escaped_grayback;
pub mod esteemed_hierophant;
pub mod evelynn_entrancing;
pub mod evershade_stalker;
pub mod existential_dread;
pub mod experimental_hexplate;
pub mod eye_of_the_herald;
pub mod ezreal_dashing;
pub mod ezreal_prodigal_explorer;
pub mod ezreal_prodigy;
pub mod facebreaker;
pub mod factory_recall;
pub mod fading_memories;
pub mod fae_dragon;
pub mod fae_porter;
pub mod faithful_manufactor;
pub mod fallen_feline;
pub mod falling_comet;
pub mod falling_star;
pub mod fate_weaver;
pub mod feral_strength;
pub mod ferrous_forerunner;
pub mod field_musicians;
pub mod fight_or_flight;
pub mod find_your_center;
pub mod fiora_grand_duelist;
pub mod fiora_peerless;
pub mod fiora_victorious;
pub mod fiora_worthy;
pub mod first_mate;
pub mod fizz_trickster;
pub mod flame_chompers;
pub mod flurry_of_blades;
pub mod flurry_of_feathers;
pub mod forbidding_waste;
pub mod forecaster;
pub mod forge_of_the_fluft;
pub mod forge_of_the_future;
pub mod forgefire_cape;
pub mod forgotten_library;
pub mod forgotten_monument;
pub mod forgotten_relic;
pub mod forgotten_signpost;
pub mod forsaken_baccai;
pub mod fortified_position;
pub mod fox_fire;
pub mod fresh_beans;
pub mod fretful_feline;
pub mod friendship;
pub mod frigid_jewel;
pub mod frigid_touch;
pub mod frisky_hunter;
pub mod frostcoat_cub;
pub mod frostcoat_mother;
pub mod frozen_fortress;
pub mod galio_indefatigable;
pub mod gangplank_naval;
pub mod garbage_grabber;
pub mod gardens_of_becoming;
pub mod gearhead;
pub mod gem_jammer;
pub mod gemcraft_seer;
pub mod gemhand_hunter;
pub mod generic;
pub mod gentle_gemdragon;
pub mod get_excited;
pub mod glasc_mixologist;
pub mod glowstone;
pub mod gold;
pub mod grand_strategem;
pub mod grim_apothecary;
pub mod grim_resolve;
pub mod grove_of_the_god_willow;
pub mod grumpy_rockbear;
pub mod guardian_angel;
pub mod guardian_of_the_passage;
pub mod guards;
pub mod guerilla_warfare;
pub mod gust;
pub mod gust_monk;
pub mod gustwalker;
pub mod gutter_palace;
pub mod guttural_roar;
pub mod hall_of_legends;
pub mod hallowed_tomb;
pub mod hand_hammer;
pub mod hard_bargain;
pub mod harnessed_dragon;
pub mod harpoon_squad;
pub mod heart_of_dark_ice;
pub mod heedless_resurrection;
pub mod heimerdinger_inventor;
pub mod heisho_shell_of_the_world;
pub mod helm_of_suppression;
pub mod herald_of_scales;
pub mod herald_of_spring;
pub mod here_to_help;
pub mod heroic_charge;
pub mod hexdrinker;
pub mod hextech_anomaly;
pub mod hextech_disc;
pub mod hextech_formula;
pub mod hextech_gauntlets;
pub mod hextech_ray;
pub mod hidden_blade;
pub mod honest_broker;
pub mod honeyfruit;
pub mod horns_of_the_dragon;
pub mod hostile_takeover;
pub mod hungry_wolf;
pub mod hunters_machete;
pub mod hwei_brooding_painter;
pub mod iascylla;
pub mod icathian_rain;
pub mod icevale_archer;
pub mod illaoi_prophet_of_the_great_kraken;
pub mod immortal_phoenix;
pub mod imperial_decree;
pub mod imposing_challenger;
pub mod inferna;
pub mod insightful_investigator;
pub mod invert_timelines;
pub mod inviolus_vox;
pub mod irelia_blade_dancer;
pub mod irelia_fervent;
pub mod irelia_graceful;
pub mod iron_ballista;
pub mod irresistible_faefolk;
pub mod isolate;
pub mod iterative_design;
pub mod ivern_friend_to_all;
pub mod ivern_green_father;
pub mod ivern_nurturer;
pub mod jae_medarda;
pub mod jagged_cutlass;
pub mod janna_savior;
pub mod jaull_fish;
pub mod jax_grandmaster_at_arms;
pub mod jax_unmatched;
pub mod jax_unrelenting;
pub mod jayce_brilliant_inventor;
pub mod jayce_defender_of_tomorrow;
pub mod jayce_hammer_in_hand;
pub mod jayce_man_of_progress;
pub mod jeweled_colossus;
pub mod jhin_meticulous_killer;
pub mod jhin_murderous_artist;
pub mod jhin_virtuoso;
pub mod jinx_demolitionist;
pub mod jinx_loose_cannon;
pub mod jinx_rebel;
pub mod kadregrin_the_infernal;
pub mod kai_sa_daughter_of_the_void;
pub mod kai_sa_evolutionary;
pub mod kai_sa_survivor;
pub mod karma_channeler;
pub mod karthus_eternal;
pub mod katarina_reckless;
pub mod kato_the_arm;
pub mod kayle_justified;
pub mod kayn_unleashed;
pub mod keeper_of_law;
pub mod keeper_of_masks;
pub mod keepers_verdict;
pub mod kennen_keeper_of_balance;
pub mod kennen_storm_of_shuriken;
pub mod kha_zix_evolving_hunter;
pub mod kha_zix_mutating_horror;
pub mod kha_zix_voidreaver;
pub mod kharox;
pub mod ki_barrier;
pub mod kings_edict;
pub mod kinkou_initiate;
pub mod kinkou_lifeblade;
pub mod kinkou_monk;
pub mod kinkou_temple;
pub mod kog_maw_caustic;
pub mod kraken_hunter;
pub mod lacerate;
pub mod last_breath;
pub mod last_rites;
pub mod last_stand;
pub mod laurent_bladekeeper;
pub mod laurent_duelist;
pub mod leblanc_deceiver;
pub mod leblanc_everywhere_at_once;
pub mod leblanc_fragmented;
pub mod lecturing_yordle;
pub mod lee_sin_ascetic;
pub mod lee_sin_blind_monk;
pub mod lee_sin_centered;
pub mod legion_marauder;
pub mod legion_quartermaster;
pub mod legion_rearguard;
pub mod leona_determined;
pub mod leona_radiant_dawn;
pub mod leona_zealot;
pub mod lightning_rush;
pub mod lillia_bashful_bloom;
pub mod lillia_fae_fawn;
pub mod lillia_protector_of_dreams;
pub mod lilting_lullaby;
pub mod lonely_poro;
pub mod long_sword;
pub mod lord_broadmane;
pub mod lotus_trap;
pub mod loyal_poro;
pub mod loyal_pup;
pub mod lucian_gunslinger;
pub mod lucian_merciless;
pub mod lucian_purifier;
pub mod lunar_boon;
pub mod machine_evangel;
pub mod maddened_marauder;
pub mod maduli_the_gatekeeper;
pub mod mageseeker_investigator;
pub mod mageseeker_warden;
pub mod magma_wurm;
pub mod malzahar_fanatic;
pub mod marai_spire;
pub mod marching_orders;
pub mod masa_crashing_thunder;
pub mod mask_mother;
pub mod mask_of_foresight;
pub mod master_bingwen;
pub mod master_yi_tempered;
pub mod master_yi_unstoppable;
pub mod master_yi_wuju_bladesman;
pub mod master_yi_wuju_master;
pub mod meditation;
pub mod mega_mech;
pub mod megatusk;
pub mod mel_defiant_soul;
pub mod mel_newly_awakened;
pub mod mel_souls_reflection;
pub mod mesmerize;
pub mod minah_swiftfoot;
pub mod mindsplitter;
pub mod minefield;
pub mod minotaur_reckoner;
pub mod mirror_image;
pub mod mischievous_marai;
pub mod miss_fortune_bounty_hunter;
pub mod miss_fortune_buccaneer;
pub mod miss_fortune_captain;
pub mod mister_root;
pub mod mistfall;
pub mod mobilize;
pub mod monastery_of_hirana;
pub mod monch;
pub mod monster_harpoon;
pub mod moonfall;
pub mod moonlight_affliction;
pub mod morbid_return;
pub mod morgana_vindictive;
pub mod mosstomper;
pub mod mountain_drake;
pub mod mournful_witness;
pub mod mushroom_pouch;
pub mod mutated_mouser;
pub mod mystic_poro;
pub mod mystic_reversal;
pub mod mystic_vortex;
pub mod nami_headstrong;
pub mod nasus_ascended;
pub mod nasus_curator_of_the_sands;
pub mod nasus_guardian_of_knowledge;
pub mod navori_fighting_pit;
pub mod navori_scout;
pub mod needlessly_large_yordle;
pub mod nidalee_cat_form;
pub mod nilah_joyful_ascetic;
pub mod nocturne_horrifying;
pub mod not_so_fast;
pub mod noxian_demolitionist;
pub mod noxian_drummer;
pub mod noxian_emissary;
pub mod noxian_guillotine;
pub mod noxus_hopeful;
pub mod noxus_saboteur;
pub mod oasis_raider;
pub mod obelisk_of_power;
pub mod ocean_drake;
pub mod ol_poro;
pub mod on_the_hunt;
pub mod onslaught;
pub mod orb_of_regret;
pub mod ornn_blacksmith;
pub mod ornn_fire_below_the_mountain;
pub mod ornn_forge_god;
pub mod ornns_forge;
pub mod otterpus;
pub mod overt_operation;
pub mod overzealous_fan;
pub mod pack_of_wonders;
pub mod pakaa_cub;
pub mod pakaa_protector;
pub mod party_favors;
pub mod patched_porobot;
pub mod peak_guardian;
pub mod pendulum_blade;
pub mod perched_grimwyrm;
pub mod perfect_execution;
pub mod petal_pixie;
pub mod petricite_monument;
pub mod petty_officer;
pub mod pickpocket;
pub mod piercing_light;
pub mod piltovan_forge;
pub mod pirates_haven;
pub mod pit_crew;
pub mod pit_rookie;
pub mod platewyrm_egg;
pub mod playful_phantom;
pub mod plaza_guardian;
pub mod plundering_poro;
pub mod poppy_defender_of_the_meek;
pub mod poppy_keeper_of_the_hammer;
pub mod poppy_paragon;
pub mod poro_herder;
pub mod poro_snax;
pub mod portal_rescue;
pub mod possession;
pub mod pouty_poro;
pub mod power_nexus;
pub mod prelude;
pub mod premonition;
pub mod prepared_neophyte;
pub mod primal_strength;
pub mod prize_of_progress;
pub mod production_surge;
pub mod profiteer;
pub mod progress_day;
pub mod promising_future;
pub mod protective_sands;
pub mod public_execution;
pub mod punch_first;
pub mod punching_poro;
pub mod pyke_bloodharbor_ripper;
pub mod pyke_dockside_butcher;
pub mod pyke_returned;
pub mod qiyana_victorious;
pub mod questionable_tome;
pub mod rabadons_deathcrown;
pub mod rage_amplifier;
pub mod raging_firebrand;
pub mod raging_soul;
pub mod rally_the_troops;
pub mod rampage;
pub mod ravenbloom_conservatory;
pub mod ravenbloom_prefect;
pub mod ravenbloom_student;
pub mod ravenborn_tome;
pub mod reavers_row;
pub mod rebuke;
pub mod rebuttal;
pub mod reckoners_arena;
pub mod recurve_bow;
pub mod red_brambleback;
pub mod reinforce;
pub mod rek_sai_breacher;
pub mod rek_sai_swarm_queen;
pub mod rek_sai_void_burrower;
pub mod relentless_pursuit;
pub mod rell_magnetic;
pub mod reluctant_leader;
pub mod renata_glasc_chem_baroness;
pub mod renata_glasc_industrialist;
pub mod renata_glasc_mastermind;
pub mod renekton_brute;
pub mod renekton_butcher_of_the_sands;
pub mod renekton_rage_fueled;
pub mod rengar_pouncing;
pub mod rengar_pridestalker;
pub mod rengar_trophy_hunter;
pub mod rengar_unseen;
pub mod repair_specialist;
pub mod repulse;
pub mod resonating_strike;
pub mod retreat;
pub mod revna_the_lorekeeper;
pub mod rhasa_the_sunderer;
pub mod ribbon_dancer;
pub mod ride_the_wind;
pub mod rift_herald;
pub mod right_of_conquest;
pub mod riposte;
pub mod rippers_bay;
pub mod riptide_rex;
pub mod risen_altar;
pub mod riven_shattered;
pub mod rocket_barrage;
pub mod rockfall_path;
pub mod royal_entourage;
pub mod royal_guard;
pub mod ruin_runner;
pub mod ruined_rex;
pub mod rumble_hotheaded;
pub mod rumble_mechanized_menace;
pub mod rumble_scrapper;
pub mod rune_prison;
pub mod ruthless_strike;
pub mod sabotage;
pub mod sacred_protector;
pub mod sacred_shears;
pub mod sacrifice;
pub mod safety_inspector;
pub mod sai_scout;
pub mod salvage;
pub mod sanction;
pub mod sand_soldier;
pub mod sandshifter;
pub mod sandstone_chimera;
pub mod sandswept_tomb;
pub mod scorchclaw;
pub mod scrapheap;
pub mod scrapyard_champion;
pub mod scrutinizing_sergeant;
pub mod scryers_bloom;
pub mod scuttle_crab;
pub mod sea_monkey;
pub mod seal_of_discord;
pub mod seal_of_focus;
pub mod seal_of_insight;
pub mod seal_of_rage;
pub mod seal_of_strength;
pub mod seal_of_unity;
pub mod seat_of_power;
pub mod sentinel_adept;
pub mod serene_ascetic;
pub mod serrated_dirk;
pub mod sett_brawler;
pub mod sett_kingpin;
pub mod sett_the_boss;
pub mod shadow;
pub mod shadow_assassin;
pub mod shadow_clone;
pub mod shadow_dash;
pub mod shadow_fiend;
pub mod shadow_order_disciple;
pub mod shadow_temple;
pub mod shadow_watcher;
pub mod shadowblade_lurker;
pub mod shadows_call;
pub mod shadows_of_the_past;
pub mod shady_spectacles;
pub mod shakedown;
pub mod shard_of_undoing;
pub mod sharkling;
pub mod shen_eye_of_twilight;
pub mod shen_kinkou;
pub mod shen_leader_of_the_kinkou_order;
pub mod shen_scourge_of_shadows;
pub mod shepherds_heirloom;
pub mod shipyard_skulker;
pub mod shock_blast;
pub mod show_of_strength;
pub mod showstopper;
pub mod shurelyas_requiem;
pub mod shuriken_flip;
pub mod sigil_of_the_storm;
pub mod simian_ancestor;
pub mod singularity;
pub mod sinister_poro;
pub mod siphon_power;
pub mod siphoning_strike;
pub mod sivir_ambitious;
pub mod sivir_battle_mistress;
pub mod sivir_mercenary;
pub mod sky_cruiser;
pub mod sky_splitter;
pub mod skyfall_of_areion;
pub mod skyward_strike;
pub mod smite;
pub mod smoke_and_mirrors;
pub mod smoke_screen;
pub mod sneaky_deckhand;
pub mod soaring_scout;
pub mod solari_chief;
pub mod solari_shieldbearer;
pub mod solari_shrine;
pub mod solari_sunhawk;
pub mod sona_harmonious;
pub mod soraka_wanderer;
pub mod soul_harvest;
pub mod soul_shepherd;
pub mod soul_sword;
pub mod soulgorger;
pub mod soulspinner;
pub mod spectral_centaur;
pub mod spectral_matron;
pub mod spiderling;
pub mod spinning_axe;
pub mod spirit_wheel;
pub mod spirits_refuge;
pub mod spoils_of_war;
pub mod sprite_burst;
pub mod sprite_call;
pub mod sprite_fountain;
pub mod sprite_mother;
pub mod sprite_queen;
pub mod square_up;
pub mod stacked_deck;
pub mod stalking_wolf;
pub mod stalwart_poro;
pub mod stand_united;
pub mod star_crossed;
pub mod star_spring;
pub mod stare_down;
pub mod stargazer;
pub mod starhound;
pub mod startipped_peak;
pub mod stealthy_pursuer;
pub mod steel_paws;
pub mod stellacorn_herder;
pub mod steraks_gage;
pub mod stormbringer;
pub mod stormclaw_ursine;
pub mod strike_down;
pub mod stupefy;
pub mod sudden_storm;
pub mod sumpworks_map;
pub mod sun_disc;
pub mod sunken_temple;
pub mod sunlit_guardian;
pub mod super_mega_death_rocket;
pub mod svellsongur;
pub mod swain_visionary;
pub mod switcheroo;
pub mod symbol_of_the_solari;
pub mod syndra_transcendent;
pub mod tactical_retreat;
pub mod tail_cloaked_matriarch;
pub mod targonian_visionary;
pub mod targons_peak;
pub mod taric_protector;
pub mod tasty_faefolk;
pub mod teemo_scout;
pub mod teemo_strategist;
pub mod teemo_swift_scout;
pub mod temporal_breach;
pub mod temporal_portal;
pub mod temptation;
pub mod tentacle;
pub mod the_academy;
pub mod the_arenas_greatest;
pub mod the_candlelit_sanctum;
pub mod the_dreaming_tree;
pub mod the_grand_plaza;
pub mod the_harrowing;
pub mod the_list;
pub mod the_papertree;
pub mod the_ruination;
pub mod the_syren;
pub mod the_zero_drive;
pub mod thermo_beam;
pub mod thousand_tailed_watcher;
pub mod threshold_of_the_gray;
pub mod thrill_of_the_hunt;
pub mod thwonk;
pub mod tianna_crownguard;
pub mod tideturner;
pub mod time_warp;
pub mod tomb_raider_barbara;
pub mod tools_of_empire;
pub mod tornado_warrior;
pub mod towering_combatant;
pub mod towering_pairofant;
pub mod trapping_grounds;
pub mod traveling_merchant;
pub mod treasure_hoard;
pub mod treasure_hunter;
pub mod treasure_trove;
pub mod trevor_snoozebottom;
pub mod tricksy_tentacles;
pub mod trifarian_gloryseeker;
pub mod trifarian_war_camp;
pub mod trinity_force;
pub mod trove_golem;
pub mod trusty_ramhound;
pub mod tryndamere_barbarian;
pub mod turn_to_dust;
pub mod twilight_reveler;
pub mod twilight_shroud;
pub mod twilight_step;
pub mod twisted_fate_gambler;
pub mod udyr_wildman;
pub mod ultrasoft_poro;
pub mod unchecked_power;
pub mod undercover_agent;
pub mod undertitan;
pub mod undying_legion;
pub mod undying_loyalty;
pub mod unlicensed_armory;
pub mod unsung_hero;
pub mod unyielding_spirit;
pub mod up_from_the_deep;
pub mod upstage_comedy;
pub mod valley_of_idols;
pub mod vanguard_armory;
pub mod vanguard_captain;
pub mod vanguard_helm;
pub mod vanguard_sergeant;
pub mod vault_breaker;
pub mod vaults_of_helia;
pub mod vayne_hunter;
pub mod veiled_temple;
pub mod vengeance;
pub mod veteran_poro;
pub mod vex_apathetic;
pub mod vex_cheerless;
pub mod vex_gloomist;
pub mod vex_mocking;
pub mod vi_destructive;
pub mod vi_hotheaded;
pub mod vi_peacekeeper;
pub mod vi_piltover_enforcer;
pub mod vicious_snapjaws;
pub mod viktor_herald_of_the_arcane;
pub mod viktor_innovator;
pub mod viktor_leader;
pub mod vilemaw;
pub mod vilemaws_lair;
pub mod void_assault;
pub mod void_drone;
pub mod void_gate;
pub mod void_hatchling;
pub mod void_rush;
pub mod void_seeker;
pub mod volibear_furious;
pub mod volibear_imposing;
pub mod volibear_relentless_storm;
pub mod voracious_gromp;
pub mod wages_of_pain;
pub mod walking_roost;
pub mod wallop;
pub mod warmogs_armor;
pub mod warwick_hunter;
pub mod watchful_sentry;
pub mod whirlwind;
pub mod whiteflame_protector;
pub mod wielder_of_water;
pub mod wild_claw;
pub mod wildclaw_shaman;
pub mod wily_newtfish;
pub mod wind_and_ghosts;
pub mod wind_wall;
pub mod windsinger;
pub mod windswept_hillock;
pub mod wizened_elder;
pub mod world_atlas;
pub mod wraith_of_echoes;
pub mod wuju_apprentice;
pub mod xerath_freed;
pub mod xin_zhao_vigilant;
pub mod yasuo_remorseful;
pub mod yasuo_unforgiven;
pub mod yasuo_windrider;
pub mod yeti_brawler;
pub mod yone_blademaster;
pub mod yordle_explorer;
pub mod yordle_kennen_heart_of_the_tempest;
pub mod yuumi_magical_cat;
pub mod zaun_punk;
pub mod zaun_warrens;
pub mod zaunite_bouncer;
pub mod zed_from_the_shadows;
pub mod zed_master_of_shadows;
pub mod zed_without_a_sound;
pub mod zenith_blade;
pub mod zhonyas_hourglass;
pub mod zilean_time_mage;

pub type Item = ChainItem;

pub const KIND_UNIT: &str = "Unit";
pub const KIND_GEAR: &str = "Gear";
pub const KIND_SPELL: &str = "Spell";
pub const KIND_RUNE: &str = "Rune";
pub const KIND_LEGEND: &str = "Legend";
pub const KIND_BATTLEFIELD: &str = "Battlefield";
pub const RUNE_SUFFIX: &str = " Rune";
pub const TOKEN_SPRITE: &str = "Sprite";
pub const TOKEN_GOLD: &str = "Gold";
pub const TOKEN_SAND_SOLDIER: &str = "Sand Soldier";
pub const TOKEN_SHADOW_CLONE: &str = "Shadow Clone";
pub const TOKEN_TENTACLE: &str = "Tentacle";
pub const TAGS: &[&str] = &[
    "Ahri",
    "Akali",
    "Akshan",
    "Ambessa",
    "Anivia",
    "Annie",
    "Aphelios",
    "Ashe",
    "Azir",
    "Bandle City",
    "Bard",
    "Bilgewater",
    "Bird",
    "Blitzcrank",
    "Caitlyn",
    "Cat",
    "Darius",
    "Demacia",
    "Demon",
    "Diana",
    "Dog",
    "Dr. Mundo",
    "Dragon",
    "Draven",
    "Ekko",
    "Elite",
    "Equipment",
    "Evelynn",
    "Ezreal",
    "Fae",
    "Fiora",
    "Fizz",
    "Freljord",
    "Galio",
    "Gangplank",
    "Garen",
    "Heimerdinger",
    "Hwei",
    "Icathia",
    "Illaoi",
    "Ionia",
    "Irelia",
    "Ivern",
    "Ixtal",
    "Janna",
    "Jax",
    "Jayce",
    "Jhin",
    "Jinx",
    "Kai'Sa",
    "Karma",
    "Karthus",
    "Katarina",
    "Kathkan",
    "Kayle",
    "Kayn",
    "Kennen",
    "Kha'Zix",
    "Kog'Maw",
    "LeBlanc",
    "Lee Sin",
    "Leona",
    "Lillia",
    "Lucian",
    "Lux",
    "Malzahar",
    "Master Yi",
    "Mech",
    "Mel",
    "Miss Fortune",
    "Morgana",
    "Mount Targon",
    "Nami",
    "Nasus",
    "Nidalee",
    "Nilah",
    "Nocturne",
    "Noxus",
    "Ornn",
    "Piltover",
    "Pirate",
    "Poppy",
    "Poro",
    "Pyke",
    "Qiyana",
    "Recruit",
    "Rek'Sai",
    "Rell",
    "Renata Glasc",
    "Renekton",
    "Rengar",
    "Riven",
    "Rumble",
    "Sentinel",
    "Sett",
    "Shadow Isles",
    "Shen",
    "Shurima",
    "Sivir",
    "Sona",
    "Soraka",
    "Spider",
    "Spirit",
    "Swain",
    "Syndra",
    "Taric",
    "Teemo",
    "The Void",
    "Trifarian",
    "Tryndamere",
    "Twisted Fate",
    "Udyr",
    "Vayne",
    "Vex",
    "Vi",
    "Viktor",
    "Volibear",
    "Warwick",
    "Xerath",
    "Xin Zhao",
    "Yasuo",
    "Yone",
    "Yordle",
    "Yuumi",
    "Zaun",
    "Zed",
    "Zilean",
];
pub const TOKEN_BRUSH: &str = "Brush";
pub const TOKEN_BARON_PIT: &str = "Baron Pit";
pub const PRINT_SUFFIXES: &[&str] = &[
    "Starter",
    "Alternate Art",
    "Overnumbered",
    "Signature",
    "Metal",
    "Promo",
    "Prerelease",
    "Foil",
    "Ultimate",
    "Launch Exclusive",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Domain {
    Calm,
    Mind,
    Chaos,
    Fury,
    Body,
    Order,
}

impl Domain {
    pub const ALL: [Domain; 6] = [
        Domain::Calm,
        Domain::Mind,
        Domain::Chaos,
        Domain::Fury,
        Domain::Body,
        Domain::Order,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Domain::ALL
            .iter()
            .copied()
            .find(|domain| domain.label().eq_ignore_ascii_case(text.trim()))
    }

    pub fn label(self) -> &'static str {
        match self {
            Domain::Calm => "Calm",
            Domain::Mind => "Mind",
            Domain::Chaos => "Chaos",
            Domain::Fury => "Fury",
            Domain::Body => "Body",
            Domain::Order => "Order",
        }
    }

    pub fn code(self) -> u8 {
        Domain::ALL
            .iter()
            .position(|domain| *domain == self)
            .unwrap_or(0) as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Domain::ALL.get(usize::from(code)).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Domain(Domain),
    Rainbow,
    Own,
}

impl Power {
    pub const RAINBOW_CODE: u8 = 6;
    pub const OWN_CODE: u8 = 7;

    pub fn code(self) -> u8 {
        match self {
            Power::Domain(domain) => domain.code(),
            Power::Rainbow => Power::RAINBOW_CODE,
            Power::Own => Power::OWN_CODE,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            Power::RAINBOW_CODE => Some(Power::Rainbow),
            Power::OWN_CODE => Some(Power::Own),
            held => Domain::from_code(held).map(Power::Domain),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub energy: u8,
    pub power: &'static [Power],
}

impl Cost {
    pub const FREE: Cost = Cost {
        energy: 0,
        power: &[],
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Accelerate,
    Action,
    Reaction,
    Assault(u8),
    Shield(u8),
    Tank,
    Backline,
    Deflect(u8),
    Ganking,
    Hidden,
    Legion,
    Temporary,
    Vision,
    Deathknell,
    Equip(Cost),
    QuickDraw,
    Repeat(Cost),
    Weaponmaster,
    Hunt(u8),
    Empower(Cost),
    Flow(Cost),
    Ambush,
}

impl Keyword {
    pub fn codes(self) -> (u8, u8) {
        match self {
            Keyword::Accelerate => (0, 0),
            Keyword::Action => (1, 0),
            Keyword::Reaction => (2, 0),
            Keyword::Assault(n) => (3, n),
            Keyword::Shield(n) => (4, n),
            Keyword::Tank => (5, 0),
            Keyword::Backline => (6, 0),
            Keyword::Deflect(n) => (7, n),
            Keyword::Ganking => (8, 0),
            Keyword::Hidden => (9, 0),
            Keyword::Legion => (10, 0),
            Keyword::Temporary => (11, 0),
            Keyword::Vision => (12, 0),
            Keyword::Deathknell => (13, 0),
            Keyword::Equip(_) => (14, 0),
            Keyword::QuickDraw => (15, 0),
            Keyword::Repeat(_) => (16, 0),
            Keyword::Weaponmaster => (17, 0),
            Keyword::Hunt(n) => (18, n),
            Keyword::Empower(_) => (19, 0),
            Keyword::Flow(_) => (20, 0),
            Keyword::Ambush => (21, 0),
        }
    }

    pub fn from_codes(code: u8, arg: u8) -> Option<Self> {
        Some(match code {
            0 => Keyword::Accelerate,
            1 => Keyword::Action,
            2 => Keyword::Reaction,
            3 => Keyword::Assault(arg),
            4 => Keyword::Shield(arg),
            5 => Keyword::Tank,
            6 => Keyword::Backline,
            7 => Keyword::Deflect(arg),
            8 => Keyword::Ganking,
            9 => Keyword::Hidden,
            10 => Keyword::Legion,
            11 => Keyword::Temporary,
            12 => Keyword::Vision,
            13 => Keyword::Deathknell,
            15 => Keyword::QuickDraw,
            17 => Keyword::Weaponmaster,
            18 => Keyword::Hunt(arg),
            21 => Keyword::Ambush,
            _ => return None,
        })
    }

    pub fn same_kind(self, other: Keyword) -> bool {
        self.codes().0 == other.codes().0
    }

    pub fn cost(self) -> Option<Cost> {
        match self {
            Keyword::Equip(cost)
            | Keyword::Repeat(cost)
            | Keyword::Empower(cost)
            | Keyword::Flow(cost) => Some(cost),
            _ => None,
        }
    }

    pub fn with_cost(code: u8, cost: Cost) -> Option<Self> {
        Some(match code {
            14 => Keyword::Equip(cost),
            16 => Keyword::Repeat(cost),
            19 => Keyword::Empower(cost),
            20 => Keyword::Flow(cost),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    Sorcery,
    Action,
    Reaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Me,
    Friendly,
    You,
    Enemy,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Where {
    Any,
    Battlefield,
    FromLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Play,
    PlayFromFacedown,
    Activated(Timing),
    Move { of: Who, to: Where },
    Conquer(Who),
    Hold(Who),
    Death,
    UnitDies(Who),
    OpponentPlaysUnit,
    YouPlaySpell,
    AnyonePlaysSpell,
    Draw { nth: u8 },
    Chosen,
    Readied(Who),
    ChosenFriendly(Who),
    BeginningPhase,
    EndOfTurn,
    Reflexive,
    Attacks(Who),
    Defends(Who),
    Damaged(Who),
    Empowered,
    YouEmpower,
    Banished(Who),
    CombatWon(Who),
    CombatLost(Who),
    CombatEnded(Who),
    Activation { of: Who },
    YouPlayCard,
    UnitPlayedHere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Card,
    Seat,
    Zone,
    Item,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rel {
    Enemy,
    Friendly,
    SameControllerAs(u8),
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Any,
    Unit,
    Gear,
    Rune,
    Legend,
    Spell,
    Ability,
    ItemOnChain,
    Friendly,
    Enemy,
    AtBattlefield,
    InBase,
    Here,
    HiddenBattlefield,
    Facedown,
    SameLocationAs(u8),
    DifferentLocationFrom(u8),
    ToOrFromBaseOf(u8),
    NotSame(u8),
    SameControllerAs(u8),
    MightLessThan(u8),
    ItemTargetsOnly(u8),
    Domain(Domain),
    EnergyAtMost(u8),
    PowerAtMost(u8),
    Temporary,
    Exhausted,
    Ready,
    Empowered,
    Attached,
    Unattached,
    Attacker,
    Defender,
    InCombat,
    Movable,
    ItemTargetsFriendly,
    ItemTargets(&'static Filter),
    ItemControlledBy(Rel),
    InTrash,
    InBanishment,
    InHand,
    InChampionZone,
    Owned,
    Champion(&'static str),
    MightAtMost(u8),
    Equipment,
    SameLocationAsPicks,
    TotalMightAtMost(u8),
    DifferentLocationFromPicks,
    MovableToBase,
    ZoneWithUnits(Rel),
    InShowdown,
    InCombatWith(&'static Filter),
    ChosenByEnemyItem(&'static Filter),
    Kind(&'static str),
    Named(&'static str),
    NamedTag,
    NotSelf,
    And(&'static [Filter]),
    Or(&'static [Filter]),
    Not(&'static Filter),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelGate {
    pub xp: u8,
    pub min: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSpec {
    pub filter: Filter,
    pub min: u8,
    pub max: u8,
    pub kind: TargetKind,
    pub label: &'static str,
    pub min_at_level: Option<LevelGate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub card: u32,
    pub ability: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Done,
    Ask(Ask),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WouldDie {
    pub unit: u32,
    pub cause: Cause,
}

pub type ExtraCost = fn(&Ctx, Source) -> Cost;
pub type Condition = fn(&Ctx, &Event, Source) -> bool;
pub type Run = fn(&mut Ctx, &Item, Stage) -> Flow;
pub type Candidates = fn(&Ctx, &Item, Stage) -> Vec<TargetRef>;
pub type Viable = fn(&Ctx, &Item, TargetRef) -> bool;
pub type Usable = fn(&Ctx, Source) -> bool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeTiming {
    #[default]
    AtPlay,
    AtResume,
}

#[derive(Debug, Clone, Copy)]
pub struct ModeSpec {
    pub label: &'static str,
    pub targets: &'static [TargetSpec],
    pub run: Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfCost {
    Auto,
    Exhaust,
    KillSelf,
    Free,
    BanishTarget,
    Disempower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Once {
    #[default]
    Never,
    PerTurn,
    PerSeatPerTurn,
}

#[derive(Debug, Clone, Copy)]
pub struct Ability {
    pub trigger: Trigger,
    pub optional: bool,
    pub cost: Option<Cost>,
    pub extra: Option<ExtraCost>,
    pub condition: Option<Condition>,
    pub targets: &'static [TargetSpec],
    pub modes: &'static [ModeSpec],
    pub mode_timing: ModeTiming,
    pub run: Run,
    pub candidates: Option<Candidates>,
    pub viable: Option<Viable>,
    pub question: Option<&'static str>,
    pub label: Option<&'static str>,
    pub self_cost: SelfCost,
    pub once: Once,
    pub xp: u8,
    pub burn: u8,
    pub usable: Option<Usable>,
}

impl Ability {
    pub fn timing(&self) -> Option<Timing> {
        match self.trigger {
            Trigger::Activated(timing) => Some(timing),
            _ => None,
        }
    }
}

pub const IMPLICIT_TEMPORARY: u8 = u8::MAX;
pub const IMPLICIT_HUNT: u8 = u8::MAX - 2;
pub const IMPLICIT_FLOW: u8 = u8::MAX - 3;
pub const IMPLICIT_VISION: u8 = u8::MAX - 4;
pub const IMPLICIT_WEAPONMASTER: u8 = u8::MAX - 5;
pub const GRANTED: u8 = 128;

pub fn is_granted(index: u8) -> bool {
    (GRANTED..IMPLICIT_FLOW).contains(&index)
}

pub type Untargetable = fn(&Ctx, u32) -> bool;
pub type Applies = fn(&Ctx, u32) -> bool;
pub type AuraWhen = fn(&Ctx, u32, u32) -> bool;
pub type MightWhen = fn(&Ctx, u32, u32) -> bool;
pub type Discount = fn(&Ctx, u32, u8) -> Cost;
pub type ItemDiscount = fn(&Ctx, &ChainItem, u32) -> Cost;
pub type Suppresses = fn(&Ctx, u32, u32) -> bool;
pub type BonusDamage = fn(&Ctx, u32, u32, &Cause) -> u8;
pub type NoDamage = fn(&Ctx, u32, &Cause) -> bool;
pub type LethalDamage = fn(&Ctx, u32, u32) -> bool;
pub type ScoreVeto = fn(&Ctx, u16, u8) -> bool;
pub type ScoreReplaces = fn(&Ctx, u32, u8) -> bool;
pub type ProtectsFromCounter = fn(&Ctx, &ChainItem) -> bool;
pub type EntersAt = fn(&mut Ctx, u32, Location) -> Location;
pub type ChannelCount = fn(&Ctx, u8) -> u8;
pub type BanishesInsteadOfTrash = fn(&Ctx, u32, Option<u16>) -> bool;
pub type IgnoresTank = fn(&Ctx, u32, u8, u16) -> bool;
pub type TieRecallsAll = fn(&Ctx, u32, u8) -> bool;
pub type DeflectIgnoredHere = fn(&Ctx, &ChainItem, Option<TargetRef>, u32) -> bool;
pub type GrantsRepeat = fn(&Ctx, &ChainItem, u32) -> Option<Cost>;
pub type GrantsAccelerate = fn(&Ctx, &ChainItem, u32) -> bool;
pub type PlayLocations = fn(&Ctx, u8, u32) -> Vec<Location>;
pub type CopiedText = fn(&Ctx, u32) -> &'static [Ability];
pub type Mirror = fn(Trigger) -> Option<Trigger>;
pub type Borrowed = fn(&Ctx, u32) -> Vec<(u32, u8)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    FriendlyUnits,
    UnitsHere,
    FriendlyTokens,
    Legend,
}

#[derive(Debug, Clone, Copy)]
pub enum Grant {
    Keyword(Keyword),
    Static(Static),
    Might(i16),
    MightIf(MightWhen, i16),
    Ability(&'static [Ability]),
    Copied(CopiedText),
    Mirror(Mirror),
    Borrowed(Borrowed),
}

#[derive(Debug, Clone, Copy)]
pub enum Static {
    Untargetable(Untargetable),
    NoUnitsPlayedHere,
    IgnoresDeflect,
    WhileAttached(&'static [Grant]),
    Level(u8, &'static [Grant]),
    While(Applies, &'static [Grant]),
    Aura {
        scope: Scope,
        when: AuraWhen,
        grants: &'static [Grant],
    },
    NoMoveToBase,
    NoUnitsMoveToBase,
    NoMoveByEnemy,
    AmbushIntoEnemies,
    SelfDiscount(Discount),
    PlayDiscount(ItemDiscount),
    AbilityDiscount(ItemDiscount),
    Surcharge(ItemDiscount),
    NoCombatDamageFrom(Suppresses),
    EntersExhausted,
    BonusDamage(BonusDamage),
    NoDamage(NoDamage),
    LethalDamage(LethalDamage),
    NoScoreHere(ScoreVeto),
    OpponentsCannotScore(Applies),
    DrawInsteadOfScoring(ScoreReplaces),
    VictoryScore(i32),
    Uncounterable(Applies),
    ProtectsFromCounter(ProtectsFromCounter),
    MoveHereFromAnywhere,
    EntersAt(EntersAt),
    CounterCap {
        counter: u16,
        max: Option<i32>,
    },
    SkipsDrawPhase(Applies),
    ChannelCount(ChannelCount),
    BanishesInsteadOfTrash(BanishesInsteadOfTrash),
    IgnoresTank(IgnoresTank),
    TieRecallsAll(TieRecallsAll),
    DeflectIgnoredHere(DeflectIgnoredHere),
    EntersReady(Applies),
    ReadySuppressed {
        by_effects: Suppresses,
        by_awaken: Suppresses,
    },
    GrantsRepeat(GrantsRepeat),
    GrantsAccelerate(GrantsAccelerate),
    PlayLocations(PlayLocations),
    OnlyPlayLocations(PlayLocations),
    OpponentsPlayUnitsOnlyToBase,
}

impl Static {
    pub fn same_kind(self, other: Static) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

fn static_mentions(held: &Static, wanted: Static) -> bool {
    if held.same_kind(wanted) {
        return true;
    }
    let granted = match held {
        Static::WhileAttached(grants)
        | Static::Level(_, grants)
        | Static::While(_, grants)
        | Static::Aura { grants, .. } => *grants,
        _ => return false,
    };
    granted.iter().any(|grant| match grant {
        Grant::Static(inner) => static_mentions(inner, wanted),
        _ => false,
    })
}

pub type ReplacementApplies = fn(&Ctx, &WouldDie, Source) -> bool;
pub type ReplacementRun = fn(&mut Ctx, &WouldDie, Source);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spend {
    Exhaust,
    Kill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adds {
    pub adds: Cost,
    pub pays: Cost,
    pub spend: Spend,
    pub most: u8,
}

impl Adds {
    pub const fn exhausting(adds: Cost) -> Self {
        Self {
            adds,
            pays: Cost::FREE,
            spend: Spend::Exhaust,
            most: 1,
        }
    }

    pub const fn killing(adds: Cost) -> Self {
        Self {
            spend: Spend::Kill,
            ..Self::exhausting(adds)
        }
    }

    pub const fn paying(self, pays: Cost) -> Self {
        Self { pays, ..self }
    }

    pub const fn up_to(self, most: u8) -> Self {
        Self { most, ..self }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Paying<'a> {
    Item(&'a Item),
    Applied,
}

impl<'a> Paying<'a> {
    pub fn item(self) -> Option<&'a Item> {
        match self {
            Self::Item(item) => Some(item),
            Self::Applied => None,
        }
    }

    pub fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }
}

pub type Adder = fn(&Ctx, u8, u32, Paying) -> Option<Adds>;

#[derive(Debug, Clone, Copy)]
pub struct Replacement {
    pub applies: ReplacementApplies,
    pub run: ReplacementRun,
}

#[derive(Debug, Clone, Copy)]
pub struct Card {
    pub name: &'static str,
    pub keywords: &'static [Keyword],
    pub abilities: &'static [Ability],
    pub statics: &'static [Static],
    pub replacement: Option<Replacement>,
    pub additional: Option<Cost>,
    pub names: Option<NameKind>,
    pub kind: Option<&'static str>,
    pub adds: Option<Adder>,
}

impl Card {
    pub fn has_static(&self, wanted: Static) -> bool {
        self.statics.iter().any(|held| held.same_kind(wanted))
    }

    pub fn attached_grants(&self) -> &'static [Grant] {
        self.statics
            .iter()
            .find_map(|held| match held {
                Static::WhileAttached(grants) => Some(*grants),
                _ => None,
            })
            .unwrap_or(&[])
    }

    pub fn granted_abilities(&self) -> impl Iterator<Item = &'static Ability> {
        self.attached_grants()
            .iter()
            .filter_map(|grant| match grant {
                Grant::Ability(held) => Some(*held),
                _ => None,
            })
            .flatten()
    }

    pub fn equip_cost(&self) -> Option<Cost> {
        self.keywords.iter().find_map(|held| match held {
            Keyword::Equip(cost) => Some(*cost),
            _ => None,
        })
    }

    pub fn has_keyword(&self, wanted: Keyword) -> bool {
        self.keywords.iter().any(|held| held.same_kind(wanted))
    }

    pub fn enters_at(&self) -> Option<EntersAt> {
        self.statics.iter().find_map(|held| match held {
            Static::EntersAt(run) => Some(*run),
            _ => None,
        })
    }

    pub fn has_aura(&self) -> bool {
        self.statics
            .iter()
            .any(|held| matches!(held, Static::Aura { .. }))
    }

    pub fn play_location_grants(&self) -> impl Iterator<Item = PlayLocations> + '_ {
        self.statics.iter().filter_map(|held| match held {
            Static::PlayLocations(grant) => Some(*grant),
            _ => None,
        })
    }

    pub fn grants_play_locations(&self) -> bool {
        self.play_location_grants().next().is_some()
    }

    pub fn only_play_locations(&self) -> Option<PlayLocations> {
        self.statics.iter().find_map(|held| match held {
            Static::OnlyPlayLocations(only) => Some(*only),
            _ => None,
        })
    }

    pub fn mentions_static(&self, wanted: Static) -> bool {
        self.statics
            .iter()
            .any(|held| static_mentions(held, wanted))
    }

    pub fn is_equipment(&self) -> bool {
        self.equip_cost().is_some()
    }

    pub fn hunt(&self) -> u8 {
        self.keywords
            .iter()
            .filter_map(|held| match held {
                Keyword::Hunt(n) => Some(*n),
                _ => None,
            })
            .fold(0u8, u8::saturating_add)
    }

    pub fn flow_cost(&self) -> Option<Cost> {
        self.keywords.iter().find_map(|held| match held {
            Keyword::Flow(cost) => Some(*cost),
            _ => None,
        })
    }

    pub fn empower_cost(&self) -> Option<Cost> {
        self.keywords.iter().find_map(|held| match held {
            Keyword::Empower(cost) => Some(*cost),
            _ => None,
        })
    }
}

pub static CARDS: &[&Card] = &[
    &abandon::CARD,
    &abandoned_hall::CARD,
    &acceleration_gate::CARD,
    &acceptable_losses::CARD,
    &adaptatron::CARD,
    &affectionate_poro::CARD,
    &against_the_odds::CARD,
    &ahri_alluring::CARD,
    &ahri_inquisitive::CARD,
    &ahri_nine_tailed_fox::CARD,
    &akali_rogue_assassin::CARD,
    &akali_deadly_weapon::CARD,
    &akali_silent::CARD,
    &akshan_mischievous::CARD,
    &albus_ferros::CARD,
    &allay_eager_admirer::CARD,
    &alpha_strike::CARD,
    &alpha_wildclaw::CARD,
    &altar_of_blood::CARD,
    &altar_of_memories::CARD,
    &altar_to_unity::CARD,
    &amateur_recital::CARD,
    &ambessa_matriarch_of_war::CARD,
    &ambessa_respected_and_feared::CARD,
    &ambessa_the_wolf::CARD,
    &ancient_henge::CARD,
    &ancient_warmonger::CARD,
    &angle_shot::CARD,
    &angler_beast::CARD,
    &anivia_primal::CARD,
    &aphelios_exalted::CARD,
    &applied_researchers::CARD,
    &apprentice_mage::CARD,
    &apprentice_smith::CARD,
    &arachnoid_horror::CARD,
    &arcane_shift::CARD,
    &arena_bar::CARD,
    &arena_kingpin::CARD,
    &arise::CARD,
    &armed_assailant::CARD,
    &ashe_focused::CARD,
    &aspirants_climb::CARD,
    &aspiring_engineer::CARD,
    &assembly_rig::CARD,
    &astral_heron::CARD,
    &atakhan::CARD,
    &aurok_general::CARD,
    &ava_achiever::CARD,
    &azir_ascendant::CARD,
    &azir_emperor_of_the_sands::CARD,
    &azir_sovereign::CARD,
    &b_f_sword::CARD,
    &baccai_reaper::CARD,
    &baccai_sandspinner::CARD,
    &baccai_witherclaw::CARD,
    &back_off::CARD,
    &back_to_back::CARD,
    &back_alley_bar::CARD,
    &baited_hook::CARD,
    &bandle_soldier::CARD,
    &bandle_tree::CARD,
    &bard_mercurial::CARD,
    &baron_nashor::CARD,
    &battering_ram::CARD,
    &beast_below::CARD,
    &bellows_breath::CARD,
    &bewitching_spirit::CARD,
    &bilgewater_bully::CARD,
    &black_flame_altar::CARD,
    &black_market_broker::CARD,
    &black_rose_dignitary::CARD,
    &blade_twirler::CARD,
    &blade_of_the_ruined_king::CARD,
    &blast_cone::CARD,
    &blast_corps_cadet::CARD,
    &blastcone_fae::CARD,
    &blazing_scorcher::CARD,
    &blighted_battleaxe::CARD,
    &blind_fury::CARD,
    &blitzcrank_impassive::CARD,
    &block::CARD,
    &blood_money::CARD,
    &blood_rose::CARD,
    &blood_rush::CARD,
    &blue_sentinel::CARD,
    &bonds_of_strength::CARD,
    &bone_skewer::CARD,
    &boneshiver::CARD,
    &boots_of_swiftness::CARD,
    &bottled_constellation::CARD,
    &brazen_buccaneer::CARD,
    &breakneck_mech::CARD,
    &brittle_steel::CARD,
    &brutal_hunter::CARD,
    &brutalizer::CARD,
    &brynhir_thundersong::CARD,
    &bubble_bot::CARD,
    &buhru_captain::CARD,
    &bullet_time::CARD,
    &bushwhack::CARD,
    &caitlyn_patrolling::CARD,
    &call_to_battle::CARD,
    &call_to_glory::CARD,
    &called_shot::CARD,
    &cannon_barrage::CARD,
    &captain_farron::CARD,
    &card_sharp::CARD,
    &carnivorous_snapvine::CARD,
    &carrion_dredger::CARD,
    &cataclysmic_duel::CARD,
    &catalyst_of_aeons::CARD,
    &cemetery_attendant::CARD,
    &chakram_dancer::CARD,
    &challenge::CARD,
    &charm::CARD,
    &chemtech_cask::CARD,
    &chemtech_enforcer::CARD,
    &cithria_of_cloudfield::CARD,
    &clairvoyance::CARD,
    &clash_of_giants::CARD,
    &cleave::CARD,
    &clockwork_keeper::CARD,
    &cloth_armor::CARD,
    &cloud_drake::CARD,
    &combat_chef::CARD,
    &combat_experience::CARD,
    &commander_ledros::CARD,
    &concentrate::CARD,
    &confront::CARD,
    &conscription::CARD,
    &consult_the_past::CARD,
    &consuming_curse::CARD,
    &convergent_mutation::CARD,
    &corina_veraza::CARD,
    &corrupt_enforcer::CARD,
    &corrupted_dragon::CARD,
    &counter_strike::CARD,
    &covert_informant::CARD,
    &crackshot_corsair::CARD,
    &crescent_guardian::CARD,
    &crescent_strike::CARD,
    &crimson_pigeons::CARD,
    &crowd_favorite::CARD,
    &cruel_patron::CARD,
    &crumbling_sands::CARD,
    &cull::CARD,
    &cull_the_weak::CARD,
    &cursed_sarcophagus::CARD,
    &curtain_call::CARD,
    &daisy::CARD,
    &dame_the_despoiler::CARD,
    &dancing_grenade::CARD,
    &danger_zone::CARD,
    &dangerous_duo::CARD,
    &daring_poro::CARD,
    &darius_executioner::CARD,
    &darius_hand_of_noxus::CARD,
    &darius_trifarian::CARD,
    &dauntless_vanguard::CARD,
    &dazzling_aurora::CARD,
    &deadbloom_predator::CARD,
    &deadly_flourish::CARD,
    &death_mark::CARD,
    &death_from_below::CARD,
    &deathgrip::CARD,
    &decree_of_discord::CARD,
    &decree_of_focus::CARD,
    &decree_of_insight::CARD,
    &decree_of_rage::CARD,
    &decree_of_strength::CARD,
    &decree_of_unity::CARD,
    &defiant_dance::CARD,
    &defy::CARD,
    &demacian_diplomat::CARD,
    &deserts_call::CARD,
    &determined_sentry::CARD,
    &detonate::CARD,
    &diana_lunari::CARD,
    &diana_no_longer_human::CARD,
    &diana_scorn_of_the_moon::CARD,
    &direwing::CARD,
    &disarming_rake::CARD,
    &disciple_of_shen::CARD,
    &discipline::CARD,
    &disintegrate::CARD,
    &disposal_order::CARD,
    &divine_judgment::CARD,
    &divining_shells::CARD,
    &dominus::CARD,
    &dorans_blade::CARD,
    &dorans_ring::CARD,
    &dorans_shield::CARD,
    &double_trouble::CARD,
    &downstage_dramatics::CARD,
    &downwell::CARD,
    &dr_mundo_expert::CARD,
    &drag_under::CARD,
    &dragon_form::CARD,
    &dragon_roost::CARD,
    &dragons_rage::CARD,
    &dragonsoul_sage::CARD,
    &dramatic_visionary::CARD,
    &draven_audacious::CARD,
    &draven_glorious_executioner::CARD,
    &draven_showboat::CARD,
    &draven_vanquisher::CARD,
    &dredge_up::CARD,
    &dropboarder::CARD,
    &dune_drake::CARD,
    &dune_surfer::CARD,
    &dunebreaker::CARD,
    &dusk_rose_lab::CARD,
    &eager_apprentice::CARD,
    &eager_drakehound::CARD,
    &eclipse::CARD,
    &eclipse_dragon::CARD,
    &eclipse_herald::CARD,
    &edge_of_night::CARD,
    &ekko_recurrent::CARD,
    &elder_dragon::CARD,
    &ember_monk::CARD,
    &eminent_benefactor::CARD,
    &emperors_dais::CARD,
    &emperors_divide::CARD,
    &en_garde::CARD,
    &endless_riches::CARD,
    &energy_conduit::CARD,
    &enthralling_protector::CARD,
    &enthusiastic_promoter::CARD,
    &escaped_grayback::CARD,
    &esteemed_hierophant::CARD,
    &evelynn_entrancing::CARD,
    &evershade_stalker::CARD,
    &existential_dread::CARD,
    &experimental_hexplate::CARD,
    &eye_of_the_herald::CARD,
    &ezreal_dashing::CARD,
    &ezreal_prodigal_explorer::CARD,
    &ezreal_prodigy::CARD,
    &facebreaker::CARD,
    &factory_recall::CARD,
    &fading_memories::CARD,
    &fae_dragon::CARD,
    &fae_porter::CARD,
    &faithful_manufactor::CARD,
    &fallen_feline::CARD,
    &falling_comet::CARD,
    &falling_star::CARD,
    &fate_weaver::CARD,
    &feral_strength::CARD,
    &ferrous_forerunner::CARD,
    &field_musicians::CARD,
    &fight_or_flight::CARD,
    &find_your_center::CARD,
    &fiora_grand_duelist::CARD,
    &fiora_peerless::CARD,
    &fiora_victorious::CARD,
    &fiora_worthy::CARD,
    &first_mate::CARD,
    &fizz_trickster::CARD,
    &flame_chompers::CARD,
    &flurry_of_blades::CARD,
    &flurry_of_feathers::CARD,
    &forbidding_waste::CARD,
    &forecaster::CARD,
    &forge_of_the_fluft::CARD,
    &forge_of_the_future::CARD,
    &forgefire_cape::CARD,
    &forgotten_library::CARD,
    &forgotten_monument::CARD,
    &forgotten_relic::CARD,
    &forgotten_signpost::CARD,
    &forsaken_baccai::CARD,
    &fortified_position::CARD,
    &fox_fire::CARD,
    &fresh_beans::CARD,
    &fretful_feline::CARD,
    &friendship::CARD,
    &frigid_jewel::CARD,
    &frigid_touch::CARD,
    &frisky_hunter::CARD,
    &frostcoat_cub::CARD,
    &frostcoat_mother::CARD,
    &frozen_fortress::CARD,
    &galio_indefatigable::CARD,
    &gangplank_naval::CARD,
    &garbage_grabber::CARD,
    &gardens_of_becoming::CARD,
    &gearhead::CARD,
    &gem_jammer::CARD,
    &gemcraft_seer::CARD,
    &gemhand_hunter::CARD,
    &gentle_gemdragon::CARD,
    &get_excited::CARD,
    &glasc_mixologist::CARD,
    &glowstone::CARD,
    &gold::CARD,
    &grand_strategem::CARD,
    &grim_apothecary::CARD,
    &grim_resolve::CARD,
    &grove_of_the_god_willow::CARD,
    &grumpy_rockbear::CARD,
    &guardian_angel::CARD,
    &guardian_of_the_passage::CARD,
    &guards::CARD,
    &guerilla_warfare::CARD,
    &gust::CARD,
    &gust_monk::CARD,
    &gustwalker::CARD,
    &gutter_palace::CARD,
    &guttural_roar::CARD,
    &hall_of_legends::CARD,
    &hallowed_tomb::CARD,
    &hand_hammer::CARD,
    &hard_bargain::CARD,
    &harnessed_dragon::CARD,
    &harpoon_squad::CARD,
    &heart_of_dark_ice::CARD,
    &heedless_resurrection::CARD,
    &heimerdinger_inventor::CARD,
    &heisho_shell_of_the_world::CARD,
    &helm_of_suppression::CARD,
    &herald_of_scales::CARD,
    &herald_of_spring::CARD,
    &here_to_help::CARD,
    &heroic_charge::CARD,
    &hexdrinker::CARD,
    &hextech_anomaly::CARD,
    &hextech_disc::CARD,
    &hextech_formula::CARD,
    &hextech_gauntlets::CARD,
    &hextech_ray::CARD,
    &hidden_blade::CARD,
    &honest_broker::CARD,
    &honeyfruit::CARD,
    &horns_of_the_dragon::CARD,
    &hostile_takeover::CARD,
    &hungry_wolf::CARD,
    &hunters_machete::CARD,
    &hwei_brooding_painter::CARD,
    &iascylla::CARD,
    &icathian_rain::CARD,
    &icevale_archer::CARD,
    &illaoi_prophet_of_the_great_kraken::CARD,
    &immortal_phoenix::CARD,
    &imperial_decree::CARD,
    &imposing_challenger::CARD,
    &inferna::CARD,
    &insightful_investigator::CARD,
    &invert_timelines::CARD,
    &inviolus_vox::CARD,
    &irelia_blade_dancer::CARD,
    &irelia_fervent::CARD,
    &irelia_graceful::CARD,
    &iron_ballista::CARD,
    &irresistible_faefolk::CARD,
    &isolate::CARD,
    &iterative_design::CARD,
    &ivern_friend_to_all::CARD,
    &ivern_green_father::CARD,
    &ivern_nurturer::CARD,
    &jae_medarda::CARD,
    &jagged_cutlass::CARD,
    &janna_savior::CARD,
    &jaull_fish::CARD,
    &jax_grandmaster_at_arms::CARD,
    &jax_unmatched::CARD,
    &jax_unrelenting::CARD,
    &jayce_defender_of_tomorrow::CARD,
    &jayce_man_of_progress::CARD,
    &jayce_brilliant_inventor::CARD,
    &jayce_hammer_in_hand::CARD,
    &jeweled_colossus::CARD,
    &jhin_meticulous_killer::CARD,
    &jhin_murderous_artist::CARD,
    &jhin_virtuoso::CARD,
    &jinx_demolitionist::CARD,
    &jinx_loose_cannon::CARD,
    &jinx_rebel::CARD,
    &kadregrin_the_infernal::CARD,
    &kai_sa_daughter_of_the_void::CARD,
    &kai_sa_evolutionary::CARD,
    &kai_sa_survivor::CARD,
    &karma_channeler::CARD,
    &karthus_eternal::CARD,
    &katarina_reckless::CARD,
    &kato_the_arm::CARD,
    &kayle_justified::CARD,
    &kayn_unleashed::CARD,
    &keeper_of_law::CARD,
    &keeper_of_masks::CARD,
    &keepers_verdict::CARD,
    &kennen_keeper_of_balance::CARD,
    &kennen_storm_of_shuriken::CARD,
    &kha_zix_evolving_hunter::CARD,
    &kha_zix_mutating_horror::CARD,
    &kha_zix_voidreaver::CARD,
    &kharox::CARD,
    &ki_barrier::CARD,
    &kings_edict::CARD,
    &kinkou_initiate::CARD,
    &kinkou_lifeblade::CARD,
    &kinkou_monk::CARD,
    &kinkou_temple::CARD,
    &kog_maw_caustic::CARD,
    &kraken_hunter::CARD,
    &lacerate::CARD,
    &last_breath::CARD,
    &last_rites::CARD,
    &last_stand::CARD,
    &laurent_bladekeeper::CARD,
    &laurent_duelist::CARD,
    &leblanc_deceiver::CARD,
    &leblanc_everywhere_at_once::CARD,
    &leblanc_fragmented::CARD,
    &lecturing_yordle::CARD,
    &lee_sin_ascetic::CARD,
    &lee_sin_blind_monk::CARD,
    &lee_sin_centered::CARD,
    &legion_marauder::CARD,
    &legion_quartermaster::CARD,
    &legion_rearguard::CARD,
    &leona_determined::CARD,
    &leona_radiant_dawn::CARD,
    &leona_zealot::CARD,
    &lightning_rush::CARD,
    &lillia_bashful_bloom::CARD,
    &lillia_fae_fawn::CARD,
    &lillia_protector_of_dreams::CARD,
    &lilting_lullaby::CARD,
    &lonely_poro::CARD,
    &long_sword::CARD,
    &lord_broadmane::CARD,
    &lotus_trap::CARD,
    &loyal_poro::CARD,
    &loyal_pup::CARD,
    &lucian_gunslinger::CARD,
    &lucian_merciless::CARD,
    &lucian_purifier::CARD,
    &lunar_boon::CARD,
    &machine_evangel::CARD,
    &maddened_marauder::CARD,
    &maduli_the_gatekeeper::CARD,
    &mageseeker_investigator::CARD,
    &mageseeker_warden::CARD,
    &magma_wurm::CARD,
    &malzahar_fanatic::CARD,
    &marai_spire::CARD,
    &marching_orders::CARD,
    &masa_crashing_thunder::CARD,
    &mask_mother::CARD,
    &mask_of_foresight::CARD,
    &master_bingwen::CARD,
    &master_yi_tempered::CARD,
    &master_yi_unstoppable::CARD,
    &master_yi_wuju_bladesman::CARD,
    &master_yi_wuju_master::CARD,
    &meditation::CARD,
    &mega_mech::CARD,
    &megatusk::CARD,
    &mel_souls_reflection::CARD,
    &mel_defiant_soul::CARD,
    &mel_newly_awakened::CARD,
    &mesmerize::CARD,
    &minah_swiftfoot::CARD,
    &mindsplitter::CARD,
    &minefield::CARD,
    &minotaur_reckoner::CARD,
    &mirror_image::CARD,
    &mischievous_marai::CARD,
    &miss_fortune_bounty_hunter::CARD,
    &miss_fortune_buccaneer::CARD,
    &miss_fortune_captain::CARD,
    &mister_root::CARD,
    &mistfall::CARD,
    &mobilize::CARD,
    &monastery_of_hirana::CARD,
    &monch::CARD,
    &monster_harpoon::CARD,
    &moonfall::CARD,
    &moonlight_affliction::CARD,
    &morbid_return::CARD,
    &morgana_vindictive::CARD,
    &mosstomper::CARD,
    &mountain_drake::CARD,
    &mournful_witness::CARD,
    &mushroom_pouch::CARD,
    &mutated_mouser::CARD,
    &mystic_poro::CARD,
    &mystic_reversal::CARD,
    &mystic_vortex::CARD,
    &nami_headstrong::CARD,
    &nasus_curator_of_the_sands::CARD,
    &nasus_ascended::CARD,
    &nasus_guardian_of_knowledge::CARD,
    &navori_fighting_pit::CARD,
    &navori_scout::CARD,
    &needlessly_large_yordle::CARD,
    &nidalee_cat_form::CARD,
    &nilah_joyful_ascetic::CARD,
    &nocturne_horrifying::CARD,
    &not_so_fast::CARD,
    &noxian_demolitionist::CARD,
    &noxian_drummer::CARD,
    &noxian_emissary::CARD,
    &noxian_guillotine::CARD,
    &noxus_hopeful::CARD,
    &noxus_saboteur::CARD,
    &oasis_raider::CARD,
    &obelisk_of_power::CARD,
    &ocean_drake::CARD,
    &ol_poro::CARD,
    &on_the_hunt::CARD,
    &onslaught::CARD,
    &orb_of_regret::CARD,
    &ornn_blacksmith::CARD,
    &ornn_fire_below_the_mountain::CARD,
    &ornn_forge_god::CARD,
    &ornns_forge::CARD,
    &otterpus::CARD,
    &overt_operation::CARD,
    &overzealous_fan::CARD,
    &pack_of_wonders::CARD,
    &pakaa_cub::CARD,
    &pakaa_protector::CARD,
    &party_favors::CARD,
    &patched_porobot::CARD,
    &peak_guardian::CARD,
    &pendulum_blade::CARD,
    &perched_grimwyrm::CARD,
    &perfect_execution::CARD,
    &petal_pixie::CARD,
    &petricite_monument::CARD,
    &petty_officer::CARD,
    &pickpocket::CARD,
    &piercing_light::CARD,
    &piltovan_forge::CARD,
    &pirates_haven::CARD,
    &pit_crew::CARD,
    &pit_rookie::CARD,
    &platewyrm_egg::CARD,
    &playful_phantom::CARD,
    &plaza_guardian::CARD,
    &plundering_poro::CARD,
    &poppy_defender_of_the_meek::CARD,
    &poppy_keeper_of_the_hammer::CARD,
    &poppy_paragon::CARD,
    &poro_herder::CARD,
    &poro_snax::CARD,
    &portal_rescue::CARD,
    &possession::CARD,
    &pouty_poro::CARD,
    &power_nexus::CARD,
    &premonition::CARD,
    &prepared_neophyte::CARD,
    &primal_strength::CARD,
    &prize_of_progress::CARD,
    &production_surge::CARD,
    &profiteer::CARD,
    &progress_day::CARD,
    &promising_future::CARD,
    &protective_sands::CARD,
    &public_execution::CARD,
    &punch_first::CARD,
    &punching_poro::CARD,
    &pyke_bloodharbor_ripper::CARD,
    &pyke_dockside_butcher::CARD,
    &pyke_returned::CARD,
    &qiyana_victorious::CARD,
    &questionable_tome::CARD,
    &rabadons_deathcrown::CARD,
    &rage_amplifier::CARD,
    &raging_firebrand::CARD,
    &raging_soul::CARD,
    &rally_the_troops::CARD,
    &rampage::CARD,
    &ravenbloom_conservatory::CARD,
    &ravenbloom_prefect::CARD,
    &ravenbloom_student::CARD,
    &ravenborn_tome::CARD,
    &reavers_row::CARD,
    &rebuke::CARD,
    &rebuttal::CARD,
    &reckoners_arena::CARD,
    &recurve_bow::CARD,
    &red_brambleback::CARD,
    &reinforce::CARD,
    &rek_sai_breacher::CARD,
    &rek_sai_swarm_queen::CARD,
    &rek_sai_void_burrower::CARD,
    &relentless_pursuit::CARD,
    &rell_magnetic::CARD,
    &reluctant_leader::CARD,
    &renata_glasc_chem_baroness::CARD,
    &renata_glasc_industrialist::CARD,
    &renata_glasc_mastermind::CARD,
    &renekton_butcher_of_the_sands::CARD,
    &renekton_brute::CARD,
    &renekton_rage_fueled::CARD,
    &rengar_pouncing::CARD,
    &rengar_pridestalker::CARD,
    &rengar_trophy_hunter::CARD,
    &rengar_unseen::CARD,
    &repair_specialist::CARD,
    &repulse::CARD,
    &resonating_strike::CARD,
    &retreat::CARD,
    &revna_the_lorekeeper::CARD,
    &rhasa_the_sunderer::CARD,
    &ribbon_dancer::CARD,
    &ride_the_wind::CARD,
    &rift_herald::CARD,
    &right_of_conquest::CARD,
    &riposte::CARD,
    &rippers_bay::CARD,
    &riptide_rex::CARD,
    &risen_altar::CARD,
    &riven_shattered::CARD,
    &rocket_barrage::CARD,
    &rockfall_path::CARD,
    &royal_entourage::CARD,
    &royal_guard::CARD,
    &ruin_runner::CARD,
    &ruined_rex::CARD,
    &rumble_hotheaded::CARD,
    &rumble_mechanized_menace::CARD,
    &rumble_scrapper::CARD,
    &rune_prison::CARD,
    &ruthless_strike::CARD,
    &sabotage::CARD,
    &sacred_protector::CARD,
    &sacred_shears::CARD,
    &sacrifice::CARD,
    &safety_inspector::CARD,
    &sai_scout::CARD,
    &salvage::CARD,
    &sanction::CARD,
    &sand_soldier::CARD,
    &sandshifter::CARD,
    &sandstone_chimera::CARD,
    &sandswept_tomb::CARD,
    &scorchclaw::CARD,
    &scrapheap::CARD,
    &scrapyard_champion::CARD,
    &scrutinizing_sergeant::CARD,
    &scryers_bloom::CARD,
    &scuttle_crab::CARD,
    &sea_monkey::CARD,
    &seal_of_discord::CARD,
    &seal_of_focus::CARD,
    &seal_of_insight::CARD,
    &seal_of_rage::CARD,
    &seal_of_strength::CARD,
    &seal_of_unity::CARD,
    &seat_of_power::CARD,
    &sentinel_adept::CARD,
    &serene_ascetic::CARD,
    &serrated_dirk::CARD,
    &sett_brawler::CARD,
    &sett_kingpin::CARD,
    &sett_the_boss::CARD,
    &shadow::CARD,
    &shadow_assassin::CARD,
    &shadow_clone::CARD,
    &shadow_dash::CARD,
    &shadow_fiend::CARD,
    &shadow_order_disciple::CARD,
    &shadow_temple::CARD,
    &shadow_watcher::CARD,
    &shadows_call::CARD,
    &shadowblade_lurker::CARD,
    &shadows_of_the_past::CARD,
    &shady_spectacles::CARD,
    &shakedown::CARD,
    &shard_of_undoing::CARD,
    &sharkling::CARD,
    &shen_eye_of_twilight::CARD,
    &shen_kinkou::CARD,
    &shen_leader_of_the_kinkou_order::CARD,
    &shen_scourge_of_shadows::CARD,
    &shepherds_heirloom::CARD,
    &shipyard_skulker::CARD,
    &shock_blast::CARD,
    &show_of_strength::CARD,
    &showstopper::CARD,
    &shurelyas_requiem::CARD,
    &shuriken_flip::CARD,
    &sigil_of_the_storm::CARD,
    &simian_ancestor::CARD,
    &singularity::CARD,
    &sinister_poro::CARD,
    &siphon_power::CARD,
    &siphoning_strike::CARD,
    &sivir_ambitious::CARD,
    &sivir_battle_mistress::CARD,
    &sivir_mercenary::CARD,
    &sky_cruiser::CARD,
    &sky_splitter::CARD,
    &skyfall_of_areion::CARD,
    &skyward_strike::CARD,
    &smite::CARD,
    &smoke_screen::CARD,
    &smoke_and_mirrors::CARD,
    &sneaky_deckhand::CARD,
    &soaring_scout::CARD,
    &solari_chief::CARD,
    &solari_shieldbearer::CARD,
    &solari_shrine::CARD,
    &solari_sunhawk::CARD,
    &sona_harmonious::CARD,
    &soraka_wanderer::CARD,
    &soul_harvest::CARD,
    &soul_shepherd::CARD,
    &soul_sword::CARD,
    &soulgorger::CARD,
    &soulspinner::CARD,
    &spectral_centaur::CARD,
    &spectral_matron::CARD,
    &spiderling::CARD,
    &spinning_axe::CARD,
    &spirit_wheel::CARD,
    &spirits_refuge::CARD,
    &spoils_of_war::CARD,
    &sprite_burst::CARD,
    &sprite_call::CARD,
    &sprite_fountain::CARD,
    &sprite_mother::CARD,
    &sprite_queen::CARD,
    &square_up::CARD,
    &stacked_deck::CARD,
    &stalking_wolf::CARD,
    &stalwart_poro::CARD,
    &stand_united::CARD,
    &star_spring::CARD,
    &star_crossed::CARD,
    &stare_down::CARD,
    &stargazer::CARD,
    &starhound::CARD,
    &startipped_peak::CARD,
    &stealthy_pursuer::CARD,
    &steel_paws::CARD,
    &stellacorn_herder::CARD,
    &steraks_gage::CARD,
    &stormbringer::CARD,
    &stormclaw_ursine::CARD,
    &strike_down::CARD,
    &stupefy::CARD,
    &sudden_storm::CARD,
    &sumpworks_map::CARD,
    &sun_disc::CARD,
    &sunken_temple::CARD,
    &sunlit_guardian::CARD,
    &super_mega_death_rocket::CARD,
    &svellsongur::CARD,
    &swain_visionary::CARD,
    &switcheroo::CARD,
    &symbol_of_the_solari::CARD,
    &syndra_transcendent::CARD,
    &tactical_retreat::CARD,
    &tail_cloaked_matriarch::CARD,
    &targons_peak::CARD,
    &targonian_visionary::CARD,
    &taric_protector::CARD,
    &tasty_faefolk::CARD,
    &teemo_scout::CARD,
    &teemo_strategist::CARD,
    &teemo_swift_scout::CARD,
    &temporal_breach::CARD,
    &temporal_portal::CARD,
    &temptation::CARD,
    &tentacle::CARD,
    &the_academy::CARD,
    &the_arenas_greatest::CARD,
    &the_candlelit_sanctum::CARD,
    &the_dreaming_tree::CARD,
    &the_grand_plaza::CARD,
    &the_harrowing::CARD,
    &the_list::CARD,
    &the_papertree::CARD,
    &the_ruination::CARD,
    &the_syren::CARD,
    &the_zero_drive::CARD,
    &thermo_beam::CARD,
    &thousand_tailed_watcher::CARD,
    &threshold_of_the_gray::CARD,
    &thrill_of_the_hunt::CARD,
    &thwonk::CARD,
    &tianna_crownguard::CARD,
    &tideturner::CARD,
    &time_warp::CARD,
    &tomb_raider_barbara::CARD,
    &tools_of_empire::CARD,
    &tornado_warrior::CARD,
    &towering_combatant::CARD,
    &towering_pairofant::CARD,
    &trapping_grounds::CARD,
    &traveling_merchant::CARD,
    &treasure_hoard::CARD,
    &treasure_hunter::CARD,
    &treasure_trove::CARD,
    &trevor_snoozebottom::CARD,
    &tricksy_tentacles::CARD,
    &trifarian_gloryseeker::CARD,
    &trifarian_war_camp::CARD,
    &trinity_force::CARD,
    &trove_golem::CARD,
    &trusty_ramhound::CARD,
    &tryndamere_barbarian::CARD,
    &turn_to_dust::CARD,
    &twilight_reveler::CARD,
    &twilight_shroud::CARD,
    &twilight_step::CARD,
    &twisted_fate_gambler::CARD,
    &udyr_wildman::CARD,
    &ultrasoft_poro::CARD,
    &unchecked_power::CARD,
    &undercover_agent::CARD,
    &undertitan::CARD,
    &undying_legion::CARD,
    &undying_loyalty::CARD,
    &unlicensed_armory::CARD,
    &unsung_hero::CARD,
    &unyielding_spirit::CARD,
    &up_from_the_deep::CARD,
    &upstage_comedy::CARD,
    &valley_of_idols::CARD,
    &vanguard_armory::CARD,
    &vanguard_captain::CARD,
    &vanguard_helm::CARD,
    &vanguard_sergeant::CARD,
    &vault_breaker::CARD,
    &vaults_of_helia::CARD,
    &vayne_hunter::CARD,
    &veiled_temple::CARD,
    &vengeance::CARD,
    &veteran_poro::CARD,
    &vex_apathetic::CARD,
    &vex_cheerless::CARD,
    &vex_gloomist::CARD,
    &vex_mocking::CARD,
    &vi_destructive::CARD,
    &vi_hotheaded::CARD,
    &vi_peacekeeper::CARD,
    &vi_piltover_enforcer::CARD,
    &vicious_snapjaws::CARD,
    &viktor_herald_of_the_arcane::CARD,
    &viktor_innovator::CARD,
    &viktor_leader::CARD,
    &vilemaw::CARD,
    &vilemaws_lair::CARD,
    &void_assault::CARD,
    &void_drone::CARD,
    &void_gate::CARD,
    &void_hatchling::CARD,
    &void_rush::CARD,
    &void_seeker::CARD,
    &volibear_furious::CARD,
    &volibear_imposing::CARD,
    &volibear_relentless_storm::CARD,
    &voracious_gromp::CARD,
    &wages_of_pain::CARD,
    &walking_roost::CARD,
    &wallop::CARD,
    &warmogs_armor::CARD,
    &warwick_hunter::CARD,
    &watchful_sentry::CARD,
    &whirlwind::CARD,
    &whiteflame_protector::CARD,
    &wielder_of_water::CARD,
    &wild_claw::CARD,
    &wildclaw_shaman::CARD,
    &wily_newtfish::CARD,
    &wind_wall::CARD,
    &wind_and_ghosts::CARD,
    &windsinger::CARD,
    &windswept_hillock::CARD,
    &wizened_elder::CARD,
    &world_atlas::CARD,
    &wraith_of_echoes::CARD,
    &wuju_apprentice::CARD,
    &xerath_freed::CARD,
    &xin_zhao_vigilant::CARD,
    &yasuo_remorseful::CARD,
    &yasuo_unforgiven::CARD,
    &yasuo_windrider::CARD,
    &yeti_brawler::CARD,
    &yone_blademaster::CARD,
    &yordle_explorer::CARD,
    &yordle_kennen_heart_of_the_tempest::CARD,
    &yuumi_magical_cat::CARD,
    &zaun_punk::CARD,
    &zaun_warrens::CARD,
    &zaunite_bouncer::CARD,
    &zed_master_of_shadows::CARD,
    &zed_from_the_shadows::CARD,
    &zed_without_a_sound::CARD,
    &zenith_blade::CARD,
    &zhonyas_hourglass::CARD,
    &zilean_time_mage::CARD,
];

pub fn script_of(name: &str) -> Option<&'static Card> {
    if name.ends_with(RUNE_SUFFIX) {
        return Some(&generic::RUNE);
    }
    if let Ok(index) = CARDS.binary_search_by(|card| card.name.cmp(name)) {
        return Some(CARDS[index]);
    }
    if name == TOKEN_SPRITE {
        return Some(&generic::SPRITE);
    }
    if name == TOKEN_GOLD {
        return Some(&generic::GOLD);
    }
    if name == TOKEN_BRUSH {
        return Some(&ivern_green_father::BRUSH_TOKEN);
    }
    if name == TOKEN_BARON_PIT {
        return Some(&baron_nashor::BARON_PIT_TOKEN);
    }
    None
}

pub fn spell_names() -> Vec<&'static str> {
    CARDS
        .iter()
        .filter(|card| card.kind == Some(KIND_SPELL) && !is_token_name(card.name))
        .map(|card| card.name)
        .collect()
}

pub fn is_token_name(name: &str) -> bool {
    matches!(
        name,
        "Reflection"
            | TOKEN_SPRITE
            | TOKEN_GOLD
            | TOKEN_SAND_SOLDIER
            | TOKEN_SHADOW_CLONE
            | TOKEN_TENTACLE
            | TOKEN_BRUSH
            | TOKEN_BARON_PIT
    )
}

pub fn base_name(name: &str) -> &str {
    let trimmed = name.trim_end();
    let Some(open) = trimmed.strip_suffix(')').and_then(|head| head.rfind(" (")) else {
        return name;
    };
    let suffix = &trimmed[open + 2..trimmed.len() - 1];
    if PRINT_SUFFIXES.contains(&suffix) {
        &trimmed[..open]
    } else {
        name
    }
}

pub static PRINTED_HIDDEN: &[&str] = &[];

pub fn prints_hidden(name: &str) -> bool {
    PRINTED_HIDDEN.contains(&name)
}

pub fn resolve(face: &CardInfo) -> Option<&'static Card> {
    if face.is_hidden() {
        return None;
    }
    if let Some(script) = script_of(&face.name).or_else(|| script_of(base_name(&face.name))) {
        return Some(script);
    }
    let kind = face.kind.as_deref();
    if prints_hidden(&face.name) || prints_hidden(base_name(&face.name)) {
        return generic::hidden_for_kind(kind);
    }
    generic::for_kind(kind)
}

#[derive(Debug, Clone, Default)]
pub struct Resolved {
    scripts: Vec<(u32, &'static Card)>,
    auras: Vec<u32>,
    play_locations: Vec<u32>,
    base_locks: Vec<u32>,
}

fn aura_sources_of(scripts: &[(u32, &'static Card)]) -> Vec<u32> {
    scripts
        .iter()
        .filter(|(_, script)| script.has_aura())
        .map(|(id, _)| *id)
        .collect()
}

fn play_location_sources_of(scripts: &[(u32, &'static Card)]) -> Vec<u32> {
    scripts
        .iter()
        .filter(|(_, script)| script.grants_play_locations())
        .map(|(id, _)| *id)
        .collect()
}

fn base_lock_sources_of(scripts: &[(u32, &'static Card)]) -> Vec<u32> {
    scripts
        .iter()
        .filter(|(_, script)| script.mentions_static(Static::OpponentsPlayUnitsOnlyToBase))
        .map(|(id, _)| *id)
        .collect()
}

impl Resolved {
    pub fn of(table: &Snapshot) -> Self {
        let mut scripts: Vec<(u32, &'static Card)> = table
            .cards
            .iter()
            .filter_map(|card| resolve(card).map(|script| (card.id, script)))
            .collect();
        scripts.sort_by_key(|(id, _)| *id);
        let auras = aura_sources_of(&scripts);
        let play_locations = play_location_sources_of(&scripts);
        let base_locks = base_lock_sources_of(&scripts);
        Self {
            scripts,
            auras,
            play_locations,
            base_locks,
        }
    }

    pub fn with_script(mut self, card: u32, script: &'static Card) -> Self {
        match self.scripts.binary_search_by_key(&card, |(id, _)| *id) {
            Ok(index) => self.scripts[index].1 = script,
            Err(index) => self.scripts.insert(index, (card, script)),
        }
        self.auras = aura_sources_of(&self.scripts);
        self.play_locations = play_location_sources_of(&self.scripts);
        self.base_locks = base_lock_sources_of(&self.scripts);
        self
    }

    pub fn aura_sources(&self) -> &[u32] {
        &self.auras
    }

    pub fn play_location_sources(&self) -> &[u32] {
        &self.play_locations
    }

    pub fn base_lock_sources(&self) -> &[u32] {
        &self.base_locks
    }

    pub fn of_card(&self, card: u32) -> Option<&'static Card> {
        self.scripts
            .binary_search_by_key(&card, |(id, _)| *id)
            .ok()
            .map(|index| self.scripts[index].1)
    }

    pub fn is_generic(&self, card: u32) -> bool {
        self.of_card(card).is_some_and(generic::is_generic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const POOL_FILES: &[(&str, &str)] = &[
        (
            "lillia-house",
            include_str!("../../../riftbound/rules/pool/lillia-house.md"),
        ),
        (
            "irelia-house",
            include_str!("../../../riftbound/rules/pool/irelia-house.md"),
        ),
        (
            "lillia-jonnynick",
            include_str!("../../../riftbound/rules/pool/lillia-jonnynick.md"),
        ),
        (
            "master-yi-akame",
            include_str!("../../../riftbound/rules/pool/master-yi-akame.md"),
        ),
        (
            "nasus-thundertrees",
            include_str!("../../../riftbound/rules/pool/nasus-thundertrees.md"),
        ),
        (
            "kha-zix-hotkee",
            include_str!("../../../riftbound/rules/pool/kha-zix-hotkee.md"),
        ),
        (
            "origins",
            include_str!("../../../riftbound/rules/pool/origins.md"),
        ),
        (
            "spiritforged",
            include_str!("../../../riftbound/rules/pool/spiritforged.md"),
        ),
        (
            "unleashed",
            include_str!("../../../riftbound/rules/pool/unleashed.md"),
        ),
        (
            "vendetta",
            include_str!("../../../riftbound/rules/pool/vendetta.md"),
        ),
    ];

    const RUNES: [&str; 6] = [
        "Body Rune",
        "Calm Rune",
        "Chaos Rune",
        "Fury Rune",
        "Mind Rune",
        "Order Rune",
    ];

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Row {
        name: String,
        kind: Option<String>,
        text: String,
        line: String,
        file: &'static str,
    }

    fn parse_row(file: &'static str, line: &str) -> Option<Row> {
        let rest = line.strip_prefix("- **")?;
        let (name, rest) = rest.split_once("**")?;
        let (head, text) = rest.split_once("): ")?;
        let kind = head
            .trim_start_matches(" (")
            .split(';')
            .nth(1)
            .map(str::trim)
            .map(|kind| kind.split('/').next().unwrap_or(kind).to_string());
        Some(Row {
            name: name.to_string(),
            kind,
            text: text.to_string(),
            line: line.to_string(),
            file,
        })
    }

    fn rows_of(file: &'static str, text: &str) -> Vec<Row> {
        text.lines()
            .filter_map(|line| parse_row(file, line))
            .collect()
    }

    fn label_of(text: &str) -> &str {
        text.lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or("")
            .trim()
    }

    fn scripted_of(text: &str) -> &str {
        text.lines()
            .find_map(|line| line.strip_prefix("Scripted:"))
            .unwrap_or("")
            .trim()
    }

    fn has_deck_block(text: &str) -> bool {
        text.lines().any(|line| line.starts_with("## Deck"))
    }

    fn keyword_only(text: &str) -> bool {
        let mut rest = text.trim();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                return false;
            };
            rest = after_open[close + 1..].trim_start();
            if let Some(reminder) = rest.strip_prefix('(') {
                let Some(end) = reminder.find(')') else {
                    return false;
                };
                rest = reminder[end + 1..].trim_start();
            }
        }
        rest.is_empty()
    }

    fn deck_names(text: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut inside = false;
        let mut after_deck = false;
        for line in text.lines() {
            if line.starts_with("## Deck") {
                after_deck = true;
                continue;
            }
            if !after_deck {
                continue;
            }
            if line.starts_with("```") {
                if inside {
                    break;
                }
                inside = true;
                continue;
            }
            if !inside {
                continue;
            }
            let mut parts = line.splitn(2, ' ');
            let (Some(count), Some(name)) = (parts.next(), parts.next()) else {
                continue;
            };
            if count.parse::<u32>().is_ok() {
                names.push(name.trim().to_string());
            }
        }
        names
    }

    fn pool_rows() -> Vec<Row> {
        let mut union: Vec<Row> = Vec::new();
        for (file, text) in POOL_FILES {
            for row in rows_of(file, text) {
                if !union.iter().any(|held| held.name == row.name) {
                    union.push(row);
                }
            }
        }
        union
    }

    fn pool_names() -> Vec<String> {
        pool_rows().into_iter().map(|row| row.name).collect()
    }

    fn face_of(row: &Row) -> CardInfo {
        CardInfo {
            name: row.name.clone(),
            kind: row.kind.clone(),
            ..CardInfo::default()
        }
    }

    #[test]
    fn every_pool_file_on_disk_is_in_the_table() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../riftbound/rules/pool");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".md").map(str::to_string))
            .collect();
        on_disk.sort_unstable();
        let mut listed: Vec<String> = POOL_FILES
            .iter()
            .map(|(slug, _)| slug.to_string())
            .collect();
        listed.sort_unstable();
        assert_eq!(on_disk, listed, "every pool file on disk is in POOL_FILES");
    }

    #[test]
    fn labels_are_unique_and_the_scripted_lines_are_well_formed() {
        let mut labels: Vec<&str> = Vec::new();
        for (slug, text) in POOL_FILES {
            let label = label_of(text);
            assert!(!label.is_empty(), "{slug} has an H1 label");
            assert!(!labels.contains(&label), "{slug} repeats the label {label}");
            labels.push(label);
            let scripted = scripted_of(text);
            assert!(
                scripted == "complete" || scripted == "partial",
                "{slug}: Scripted is complete or partial, not {scripted:?}"
            );
            assert!(
                rows_of(slug, text).len() > 20,
                "{slug} parses into its card lines"
            );
        }
    }

    #[test]
    fn every_shared_card_reads_the_same_in_every_pool_file() {
        let union = pool_rows();
        for (slug, text) in POOL_FILES {
            for row in rows_of(slug, text) {
                let first = union.iter().find(|held| held.name == row.name).unwrap();
                assert_eq!(
                    row.line, first.line,
                    "{} reads differently in {slug} and {}",
                    row.name, first.file
                );
            }
        }
    }

    #[test]
    fn every_deck_block_names_only_cards_of_its_own_file() {
        let mut decks = 0;
        for (slug, text) in POOL_FILES {
            if !has_deck_block(text) {
                assert_eq!(
                    scripted_of(text),
                    "partial",
                    "{slug}: a set file without a deck block stays partial"
                );
                continue;
            }
            decks += 1;
            let names = deck_names(text);
            assert!(names.len() >= 20, "{slug}: the deck block parses");
            let rows = rows_of(slug, text);
            for name in names {
                assert!(
                    rows.iter().any(|row| row.name == name),
                    "{slug}: the deck names {name} but its Cards section does not carry it"
                );
            }
        }
        assert_eq!(decks, 6, "the six deck files carry a deck block");
    }

    #[test]
    fn every_pool_card_that_prints_hidden_resolves_to_a_script_that_carries_it() {
        let rows = pool_rows();
        assert!(rows.len() > 100, "the union covers the six decks");
        let mut printed: Vec<String> = Vec::new();
        let mut claimed: Vec<String> = Vec::new();
        for row in &rows {
            let script =
                resolve(&face_of(row)).unwrap_or_else(|| panic!("{} has no script", row.name));
            if row.text.starts_with("[Hidden]") {
                printed.push(row.name.clone());
                assert!(
                    script.has_keyword(Keyword::Hidden),
                    "737.1 · {} prints [Hidden] and the engine must read it, generic or not",
                    row.name
                );
            } else {
                assert!(
                    !script.has_keyword(Keyword::Hidden),
                    "{} does not print Hidden but its script claims it",
                    row.name
                );
            }
            if script_of(&row.name).is_some_and(|script| script.has_keyword(Keyword::Hidden)) {
                claimed.push(row.name.clone());
            }
        }
        claimed.extend(PRINTED_HIDDEN.iter().map(|name| name.to_string()));
        printed.sort_unstable();
        claimed.sort_unstable();
        assert_eq!(
            printed, claimed,
            "the pool's Hidden faces are the scripts claiming Hidden plus PRINTED_HIDDEN"
        );
        let mut sorted = PRINTED_HIDDEN.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(PRINTED_HIDDEN, sorted.as_slice());
        for name in PRINTED_HIDDEN {
            assert!(
                script_of(name).is_none(),
                "{name} has a script of its own now, so drop it from PRINTED_HIDDEN"
            );
        }
    }

    #[test]
    fn the_registry_is_sorted_and_every_scripted_name_is_in_the_pool_or_a_token() {
        let names: Vec<&str> = CARDS.iter().map(|card| card.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "CARDS is sorted by name without duplicates");
        let pool = pool_names();
        assert!(pool.len() > 100, "the pool union lists the six decks");
        for name in names {
            assert!(
                pool.iter().any(|held| held == name) || is_token_name(name),
                "{name} is scripted but not in the pool or a token"
            );
        }
        assert!(is_token_name(TOKEN_GOLD));
        assert!(is_token_name(TOKEN_SAND_SOLDIER));
        assert!(is_token_name(TOKEN_SHADOW_CLONE));
        assert!(is_token_name(TOKEN_TENTACLE));
        assert!(is_token_name(TOKEN_BRUSH));
        assert!(is_token_name(TOKEN_BARON_PIT));
        assert!(std::ptr::eq(
            script_of(TOKEN_BRUSH).unwrap(),
            &ivern_green_father::BRUSH_TOKEN
        ));
        assert!(std::ptr::eq(
            script_of(TOKEN_BARON_PIT).unwrap(),
            &baron_nashor::BARON_PIT_TOKEN
        ));
        assert!(!is_token_name("Vi"));
    }

    #[test]
    fn every_script_carries_the_kind_its_pool_line_prints_and_the_spells_are_the_name_list() {
        let rows = pool_rows();
        for card in CARDS {
            let Some(row) = rows.iter().find(|row| row.name == card.name) else {
                continue;
            };
            assert_eq!(
                card.kind,
                row.kind.as_deref(),
                "{} is scripted under the kind its pool line prints",
                card.name
            );
        }
        let spells = spell_names();
        let mut sorted = spells.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(spells, sorted, "the name list is sorted without duplicates");
        assert!(
            spells.len() > 100,
            "762: the whole catalog of spells is offered"
        );
        for row in &rows {
            let scripted = script_of(&row.name).is_some();
            let is_spell = row.kind.as_deref() == Some(KIND_SPELL);
            assert_eq!(
                spells.contains(&row.name.as_str()),
                scripted && is_spell,
                "{} is offered exactly when it is a scripted spell",
                row.name
            );
        }
        assert!(!spells.iter().any(|name| is_token_name(name)), "762.2");
    }

    #[test]
    fn complete_files_are_fully_scripted_and_partial_files_resolve_what_they_have() {
        let mut runes: Vec<String> = Vec::new();
        let mut scripted: Vec<String> = Vec::new();
        for (slug, text) in POOL_FILES {
            let complete = scripted_of(text) == "complete";
            for row in rows_of(slug, text) {
                let script =
                    resolve(&face_of(&row)).unwrap_or_else(|| panic!("{} has no script", row.name));
                if row.kind.as_deref() == Some(KIND_RUNE) {
                    assert!(
                        std::ptr::eq(script, &generic::RUNE),
                        "{} is a rune and runs on the generic rune script",
                        row.name
                    );
                    if !runes.contains(&row.name) {
                        runes.push(row.name.clone());
                    }
                    continue;
                }
                if let Some(own) = script_of(&row.name) {
                    assert_eq!(own.name, row.name, "{} resolves to its own file", row.name);
                    assert!(
                        std::ptr::eq(own, script),
                        "{} resolves through its face to its own script",
                        row.name
                    );
                    let written = !own.abilities.is_empty()
                        || !own.statics.is_empty()
                        || own.replacement.is_some()
                        || own.additional.is_some()
                        || keyword_only(&row.text);
                    assert!(
                        written || !complete,
                        "{slug} is Scripted: complete but {} is still a stub",
                        row.name
                    );
                    if !scripted.contains(&row.name) {
                        scripted.push(row.name.clone());
                    }
                } else {
                    assert!(
                        !complete,
                        "{slug} is Scripted: complete but {} still runs on the generic script",
                        row.name
                    );
                    assert!(
                        generic::is_generic(script),
                        "{} without a script of its own is generic",
                        row.name
                    );
                }
            }
        }
        runes.sort_unstable();
        assert_eq!(runes, RUNES);
        let mut registry: Vec<String> = CARDS
            .iter()
            .map(|card| card.name.to_string())
            .filter(|name| !is_token_name(name))
            .collect();
        registry.sort_unstable();
        scripted.sort_unstable();
        assert_eq!(
            registry, scripted,
            "every script in CARDS is a pool card, and every scripted pool card is in CARDS"
        );
        const M3: [&Card; 8] = [
            &akali_silent::CARD,
            &back_off::CARD,
            &charm::CARD,
            &ride_the_wind::CARD,
            &scuttle_crab::CARD,
            &stellacorn_herder::CARD,
            &treasure_hunter::CARD,
            &vex_apathetic::CARD,
        ];
        for card in M3 {
            assert!(
                std::ptr::eq(script_of(card.name).unwrap(), card),
                "{} resolves to its own script",
                card.name
            );
            assert!(
                !card.abilities.is_empty() || !card.statics.is_empty(),
                "{} carries a written ability, not a stub",
                card.name
            );
            assert!(
                !generic::is_generic(card),
                "{} is no longer the generic script",
                card.name
            );
        }
        const M4: [&Card; 13] = [
            &adaptatron::CARD,
            &dusk_rose_lab::CARD,
            &lillia_bashful_bloom::CARD,
            &lillia_fae_fawn::CARD,
            &pickpocket::CARD,
            &plundering_poro::CARD,
            &seat_of_power::CARD,
            &sprite_fountain::CARD,
            &sunken_temple::CARD,
            &targons_peak::CARD,
            &tomb_raider_barbara::CARD,
            &unsung_hero::CARD,
            &zhonyas_hourglass::CARD,
        ];
        for card in M4 {
            assert!(
                std::ptr::eq(script_of(card.name).unwrap(), card),
                "{} resolves to its own script",
                card.name
            );
            assert!(
                !generic::is_generic(card),
                "{} is no longer the generic script",
                card.name
            );
        }
        const M9_VANILLA: [&Card; 13] = [
            &angler_beast::CARD,
            &disarming_rake::CARD,
            &first_mate::CARD,
            &gust::CARD,
            &lecturing_yordle::CARD,
            &pit_rookie::CARD,
            &premonition::CARD,
            &punch_first::CARD,
            &ruin_runner::CARD,
            &sprite_burst::CARD,
            &thousand_tailed_watcher::CARD,
            &traveling_merchant::CARD,
            &zaun_warrens::CARD,
        ];
        for card in M9_VANILLA {
            assert!(
                std::ptr::eq(script_of(card.name).unwrap(), card),
                "{} resolves to its own script",
                card.name
            );
            assert!(
                !card.abilities.is_empty() || !card.statics.is_empty(),
                "{} carries a written ability, not a stub",
                card.name
            );
        }
        assert!(zhonyas_hourglass::CARD.abilities.is_empty());
        assert!(zhonyas_hourglass::CARD.replacement.is_some());
        assert!(sprite_fountain::CARD.has_keyword(Keyword::Temporary));
        assert!(sprite_fountain::CARD.has_keyword(Keyword::Deathknell));
        assert!(unsung_hero::CARD.has_keyword(Keyword::Deathknell));
        assert!(lillia_fae_fawn::CARD.has_keyword(Keyword::Accelerate));
        assert!(back_off::CARD.has_keyword(Keyword::Action));
        assert!(back_off::CARD.has_keyword(Keyword::Hidden));
        assert!(ride_the_wind::CARD.has_keyword(Keyword::Action));
        assert!(vex_apathetic::CARD.has_keyword(Keyword::Deflect(1)));
        assert!(scuttle_crab::CARD.has_keyword(Keyword::Deathknell));
        assert!(defy::CARD.has_keyword(Keyword::Reaction));
        assert!(rebuke::CARD.has_keyword(Keyword::Action));
        assert!(!discipline::CARD.has_keyword(Keyword::Action));
        assert!(lecturing_yordle::CARD.has_keyword(Keyword::Tank));
        assert!(punch_first::CARD.has_keyword(Keyword::Action));
        assert!(premonition::CARD.has_keyword(Keyword::Reaction));
        assert!(gust::CARD.has_keyword(Keyword::Reaction));
        assert!(thousand_tailed_watcher::CARD.has_keyword(Keyword::Accelerate));
        assert!(ruin_runner::CARD.has_static(Static::Untargetable(|_, _| true)));
    }

    #[test]
    fn the_origins_set_file_is_partial_lists_every_base_print_and_its_stubs_carry_the_printed_keywords(
    ) {
        let (_, text) = POOL_FILES
            .iter()
            .find(|(slug, _)| *slug == "origins")
            .unwrap();
        assert_eq!(label_of(text), "Origins (OGN)");
        assert_eq!(scripted_of(text), "partial");
        assert!(!has_deck_block(text));
        let rows = rows_of("origins", text);
        assert_eq!(rows.len(), 298);
        assert!(rows.iter().all(|row| row.line.contains("** (ogn-")));
        let mut kinds: Vec<&str> = rows.iter().filter_map(|row| row.kind.as_deref()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(
            kinds,
            ["Battlefield", "Gear", "Legend", "Rune", "Spell", "Unit"]
        );
        const SEAM_STUBS: [&str; 22] = [
            "Bandle Tree",
            "Commander Ledros",
            "Cruel Patron",
            "Darius - Hand of Noxus",
            "Eclipse Herald",
            "Energy Conduit",
            "Flame Chompers",
            "Immortal Phoenix",
            "Jinx - Rebel",
            "Kai'Sa - Daughter of the Void",
            "Karma - Channeler",
            "Karthus - Eternal",
            "Kraken Hunter",
            "Mistfall",
            "Nocturne - Horrifying",
            "Noxus Saboteur",
            "Seal of Discord",
            "Seal of Focus",
            "Seal of Insight",
            "Seal of Rage",
            "Seal of Strength",
            "Seal of Unity",
        ];
        let mut stubs: Vec<&str> = rows
            .iter()
            .filter(|row| row.kind.as_deref() != Some(KIND_RUNE))
            .filter(|row| !row.line.contains("; Unit/Token;"))
            .filter(|row| {
                script_of(&row.name).is_some_and(|script| {
                    script.abilities.is_empty()
                        && script.statics.is_empty()
                        && script.replacement.is_none()
                        && script.additional.is_none()
                })
            })
            .filter(|row| !keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        stubs.sort_unstable();
        assert_eq!(
            stubs, SEAM_STUBS,
            "every stub with rules text is a card whose whole text waits on an engine seam"
        );
        let vanillas = rows
            .iter()
            .filter(|row| row.kind.as_deref() != Some(KIND_RUNE))
            .filter(|row| !row.line.contains("; Unit/Token;"))
            .filter(|row| keyword_only(&row.text))
            .count();
        assert_eq!(vanillas, 16, "fifteen vanilla units and Shen - Kinkou");
        assert!(script_of("Recruit (271) // Buff").is_none());
        assert!(std::ptr::eq(
            resolve(&CardInfo {
                name: "Sprite (274) // Buff".into(),
                kind: Some(KIND_UNIT.into()),
                ..CardInfo::default()
            })
            .unwrap(),
            &generic::UNIT
        ));
        let shen = script_of("Shen - Kinkou").unwrap();
        assert!(shen.has_keyword(Keyword::Reaction));
        assert!(shen.has_keyword(Keyword::Shield(2)));
        assert!(shen.has_keyword(Keyword::Tank));
        assert_eq!(shen.keywords.len(), 3);
        assert!(script_of("Pakaa Cub").unwrap().has_keyword(Keyword::Hidden));
        assert!(script_of("Volibear - Furious")
            .unwrap()
            .has_keyword(Keyword::Deflect(2)));
        assert!(script_of("Cleave").unwrap().has_keyword(Keyword::Action));
        assert!(
            !script_of("Cleave")
                .unwrap()
                .has_keyword(Keyword::Assault(3)),
            "a keyword the text grants is not the card's own"
        );
        assert!(
            !script_of("Noxus Saboteur")
                .unwrap()
                .has_keyword(Keyword::Hidden),
            "a keyword the text mentions is not the card's own"
        );
        assert!(script_of("Mega-Mech").unwrap().keywords.is_empty());
        assert!(keyword_only(""));
        assert!(keyword_only(
            "[Shield] (+1 :rb_might: while I'm a defender.)[Tank] (I must be assigned combat damage first.)"
        ));
        assert!(keyword_only(
            "[Hidden] (Hide now for :rb_rune_rainbow: to react with later for :rb_energy_0:.)"
        ));
        assert!(!keyword_only(
            "[Deathknell] — Draw 1. (When I die, get the effect.)"
        ));
        assert!(!keyword_only("When you play me, draw 1."));
    }

    #[test]
    fn the_spiritforged_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams(
    ) {
        let (_, text) = POOL_FILES
            .iter()
            .find(|(slug, _)| *slug == "spiritforged")
            .unwrap();
        assert_eq!(label_of(text), "Spiritforged (SFD)");
        assert_eq!(scripted_of(text), "partial");
        assert!(!has_deck_block(text));
        let rows = rows_of("spiritforged", text);
        assert_eq!(rows.len(), 222);
        assert!(rows.iter().all(|row| row.line.contains("** (sfd-")));
        let mut kinds: Vec<&str> = rows.iter().filter_map(|row| row.kind.as_deref()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds, ["Battlefield", "Gear", "Legend", "Spell", "Unit"]);
        let token = rows
            .iter()
            .find(|row| row.line.contains("; Gear/Token;"))
            .unwrap();
        assert_eq!(token.name, "Gold // Buff");
        assert!(script_of(&token.name).is_none());
        assert!(std::ptr::eq(
            resolve(&face_of(token)).unwrap(),
            &generic::GEAR
        ));
        for row in rows
            .iter()
            .filter(|row| !row.line.contains("; Gear/Token;"))
        {
            let script = script_of(&row.name)
                .unwrap_or_else(|| panic!("{} has a card file of its own", row.name));
            assert_eq!(script.name, row.name);
            assert_eq!(
                script.has_keyword(Keyword::Hidden),
                row.text.starts_with("[Hidden]"),
                "{} carries Hidden exactly when it prints it first",
                row.name
            );
        }
        let equipment: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind.as_deref() == Some(KIND_GEAR))
            .filter(|row| row.text.contains("[Equip]"))
            .collect();
        assert_eq!(equipment.len(), 31);
        for row in &equipment {
            let script = script_of(&row.name).unwrap();
            assert!(script.is_equipment(), "{} carries its Equip cost", row.name);
            assert!(
                row.text.ends_with(":rb_might:"),
                "{} ends with the attached might badge",
                row.name
            );
        }
        let axe = script_of("Spinning Axe").unwrap();
        assert!(axe.has_keyword(Keyword::QuickDraw));
        assert!(axe.has_keyword(Keyword::Temporary));
        assert_eq!(
            axe.equip_cost().map(|cost| cost.power),
            Some(&[Power::Rainbow][..])
        );
        let rites = script_of("Last Rites").unwrap();
        assert_eq!(
            rites.equip_cost().map(|cost| cost.power),
            Some(&[Power::Domain(Domain::Chaos)][..])
        );
        let drive = script_of("The Zero Drive").unwrap();
        assert_eq!(drive.equip_cost().map(|cost| cost.energy), Some(1));
        let yordle = script_of("Needlessly Large Yordle").unwrap();
        assert!(yordle.has_keyword(Keyword::Shield(5)));
        assert!(yordle.has_keyword(Keyword::Tank));
        let rengar = script_of("Rengar - Pouncing").unwrap();
        assert!(rengar.has_keyword(Keyword::Reaction));
        assert!(rengar.has_keyword(Keyword::Assault(2)));
        let rush = script_of("Blood Rush").unwrap();
        assert!(rush.has_keyword(Keyword::Action));
        assert_eq!(rush.keywords.len(), 2);
        assert!(matches!(
            rush.keywords[1],
            Keyword::Repeat(Cost {
                energy: 1,
                power: &[]
            })
        ));
        assert!(
            !rush.has_keyword(Keyword::Assault(2)),
            "a keyword the text grants is not the card's own"
        );
        let ornn = script_of("Ornn - Forge God").unwrap();
        assert!(ornn.has_keyword(Keyword::Deflect(2)));
        assert!(ornn.has_keyword(Keyword::Weaponmaster));
        assert!(script_of("Laurent Bladekeeper")
            .unwrap()
            .has_keyword(Keyword::Ganking));
        assert!(script_of("Soraka - Wanderer")
            .unwrap()
            .has_keyword(Keyword::Backline));
        assert!(
            !script_of("Breakneck Mech")
                .unwrap()
                .has_keyword(Keyword::Deflect(1)),
            "a keyword the text grants is not the card's own"
        );
        assert!(
            !script_of("Hextech Anomaly")
                .unwrap()
                .has_keyword(Keyword::Reaction),
            "an ability's timing is not the card's keyword"
        );
        assert!(
            !script_of("Ezreal - Dashing")
                .unwrap()
                .has_keyword(Keyword::Action),
            "an ability's timing is not the card's keyword"
        );
        assert!(script_of("Forgefire Cape").unwrap().keywords.len() == 1);
        assert!(script_of("Petricite Monument")
            .unwrap()
            .has_keyword(Keyword::Temporary));
        assert!(script_of("Gem Jammer").unwrap().keywords.is_empty());
        const SEAM_STUBS: [&str; 10] = [
            "Ancient Henge",
            "Aphelios - Exalted",
            "Fiora - Worthy",
            "Hextech Anomaly",
            "Jax - Unmatched",
            "Jax - Unrelenting",
            "Legion Quartermaster",
            "Ornn - Fire Below the Mountain",
            "Simian Ancestor",
            "Void Hatchling",
        ];
        let mut stubs: Vec<&str> = rows
            .iter()
            .filter(|row| !row.line.contains("; Gear/Token;"))
            .filter(|row| {
                script_of(&row.name).is_some_and(|script| {
                    script.abilities.is_empty()
                        && script.statics.is_empty()
                        && script.replacement.is_none()
                        && script.additional.is_none()
                })
            })
            .filter(|row| !keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        stubs.sort_unstable();
        assert_eq!(
            stubs, SEAM_STUBS,
            "every stub with rules text is a card whose whole text waits on an engine seam"
        );
        let mut vanillas: Vec<&str> = rows
            .iter()
            .filter(|row| !row.line.contains("; Gear/Token;"))
            .filter(|row| keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        vanillas.sort_unstable();
        assert_eq!(
            vanillas,
            [
                "Armed Assailant",
                "Combat Chef",
                "Laurent Bladekeeper",
                "Laurent Duelist",
                "Master Bingwen",
                "Navori Scout",
                "Sentinel Adept",
                "Veteran Poro",
            ],
            "eight vanillas whose printed keywords are the whole script"
        );
        assert!(script_of("Minefield")
            .unwrap()
            .abilities
            .iter()
            .any(|ability| ability.trigger == Trigger::Conquer(Who::You)));
    }

    #[test]
    fn the_unleashed_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams()
    {
        let (_, text) = POOL_FILES
            .iter()
            .find(|(slug, _)| *slug == "unleashed")
            .unwrap();
        assert_eq!(label_of(text), "Unleashed (UNL)");
        assert_eq!(scripted_of(text), "partial");
        assert!(!has_deck_block(text));
        let rows = rows_of("unleashed", text);
        assert_eq!(rows.len(), 219);
        assert!(rows.iter().all(|row| row.line.contains("** (unl-")));
        assert!(rows.iter().all(|row| !row.line.contains("/Token;")));
        let mut kinds: Vec<&str> = rows.iter().filter_map(|row| row.kind.as_deref()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds, ["Battlefield", "Gear", "Legend", "Spell", "Unit"]);
        for row in &rows {
            let script = script_of(&row.name)
                .unwrap_or_else(|| panic!("{} has a card file of its own", row.name));
            assert_eq!(script.name, row.name);
            assert_eq!(
                script.has_keyword(Keyword::Hidden),
                row.text.starts_with("[Hidden]"),
                "{} carries Hidden exactly when it prints it first",
                row.name
            );
        }
        let equipment: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind.as_deref() == Some(KIND_GEAR))
            .filter(|row| row.text.contains("[Equip]"))
            .collect();
        assert_eq!(equipment.len(), 5);
        for row in &equipment {
            let script = script_of(&row.name).unwrap();
            assert!(script.is_equipment(), "{} carries its Equip cost", row.name);
            assert!(
                row.text.ends_with(":rb_might:"),
                "{} ends with the attached might badge",
                row.name
            );
        }
        let axe = script_of("Blighted Battleaxe").unwrap();
        assert_eq!(
            axe.equip_cost(),
            Some(Cost {
                energy: 1,
                power: &[Power::Domain(Domain::Fury)]
            })
        );
        let gauntlets = script_of("Hextech Gauntlets").unwrap();
        assert_eq!(
            gauntlets.equip_cost(),
            Some(Cost {
                energy: 3,
                power: &[Power::Rainbow]
            })
        );
        assert_eq!(
            script_of("Shepherd's Heirloom").unwrap().equip_cost(),
            Some(Cost::FREE),
            "an Equip paid in XP alone carries no resource cost"
        );
        let mut vanillas: Vec<&str> = rows
            .iter()
            .filter(|row| keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        vanillas.sort_unstable();
        assert_eq!(
            vanillas,
            [
                "Inferna",
                "Mutated Mouser",
                "Rengar - Unseen",
                "Sharkling",
                "Towering Combatant",
                "Voracious Gromp",
            ],
            "six vanillas whose printed keywords are the whole script"
        );
        let rengar = script_of("Rengar - Unseen").unwrap();
        assert_eq!(
            rengar.keywords,
            [
                Keyword::Accelerate,
                Keyword::Assault(2),
                Keyword::Deflect(1),
                Keyword::Ganking
            ]
        );
        assert_eq!(script_of("Voracious Gromp").unwrap().hunt(), 3);
        assert_eq!(script_of("Herald of Spring").unwrap().hunt(), 1);
        let dread = script_of("Existential Dread").unwrap();
        assert!(dread.has_keyword(Keyword::Action));
        assert!(matches!(
            dread.keywords[1],
            Keyword::Repeat(Cost {
                energy: 2,
                power: &[]
            })
        ));
        assert!(matches!(
            script_of("Square Up").unwrap().keywords[0],
            Keyword::Repeat(Cost::FREE)
        ));
        assert!(script_of("Rift Herald")
            .unwrap()
            .has_keyword(Keyword::Deathknell));
        assert!(script_of("Undying Legion")
            .unwrap()
            .has_keyword(Keyword::Legion));
        assert_eq!(
            script_of("Poppy - Defender of the Meek").unwrap().keywords,
            [Keyword::Ambush, Keyword::Tank],
            "keyword lines printed after the rules text are still the card's own"
        );
        assert!(
            !script_of("Vault Breaker")
                .unwrap()
                .has_keyword(Keyword::Ganking),
            "a keyword the text grants is not the card's own"
        );
        assert!(
            !script_of("Mosstomper")
                .unwrap()
                .has_keyword(Keyword::Deflect(1)),
            "a keyword a Level line grants is not the card's own"
        );
        assert!(
            !script_of("Wily Newtfish")
                .unwrap()
                .has_keyword(Keyword::Ganking),
            "a keyword a condition grants is not the card's own"
        );
        for name in [
            "Forgotten Signpost",
            "Honeyfruit",
            "Dragonsoul Sage",
            "Diana - Scorn of the Moon",
            "Shadow",
        ] {
            let script = script_of(name).unwrap();
            assert!(
                !script.has_keyword(Keyword::Action) && !script.has_keyword(Keyword::Reaction),
                "{name}: an ability's timing is not the card's keyword"
            );
        }
        assert!(script_of("Divining Shells")
            .unwrap()
            .has_keyword(Keyword::Vision));
        assert!(script_of("Sumpworks Map")
            .unwrap()
            .has_keyword(Keyword::Temporary));
        assert!(script_of("Gemhand Hunter").unwrap().keywords == [Keyword::Hunt(1)]);
        const SEAM_STUBS: [&str; 11] = [
            "Diana - Lunari",
            "Diana - Scorn of the Moon",
            "Dragonsoul Sage",
            "Jhin - Meticulous Killer",
            "LeBlanc - Everywhere At Once",
            "Mageseeker Investigator",
            "Ripper's Bay",
            "Stalking Wolf",
            "Sumpworks Map",
            "Undying Legion",
            "Zilean - Time Mage",
        ];
        let mut stubs: Vec<&str> = rows
            .iter()
            .filter(|row| {
                script_of(&row.name).is_some_and(|script| {
                    script.abilities.is_empty()
                        && script.statics.is_empty()
                        && script.replacement.is_none()
                        && script.additional.is_none()
                })
            })
            .filter(|row| !keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        stubs.sort_unstable();
        assert_eq!(
            stubs, SEAM_STUBS,
            "every stub with rules text is a card whose whole text waits on an engine seam"
        );
        assert!(std::ptr::eq(
            resolve(&CardInfo {
                name: "Baron Nashor (Ultimate)".into(),
                kind: Some(KIND_UNIT.into()),
                ..CardInfo::default()
            })
            .unwrap(),
            &baron_nashor::CARD
        ));
    }

    #[test]
    fn the_vendetta_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams() {
        let (_, text) = POOL_FILES
            .iter()
            .find(|(slug, _)| *slug == "vendetta")
            .unwrap();
        assert_eq!(label_of(text), "Vendetta (VEN)");
        assert_eq!(scripted_of(text), "partial");
        assert!(!has_deck_block(text));
        let rows = rows_of("vendetta", text);
        assert_eq!(rows.len(), 166);
        assert!(rows.iter().all(|row| row.line.contains("** (ven-")));
        assert!(rows.iter().all(|row| !row.line.contains("/Token;")));
        assert!(rows.iter().all(|row| !row.line.contains("(ven-sp")));
        let mut kinds: Vec<&str> = rows.iter().filter_map(|row| row.kind.as_deref()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds, ["Battlefield", "Gear", "Legend", "Spell", "Unit"]);
        for row in &rows {
            let script = script_of(&row.name)
                .unwrap_or_else(|| panic!("{} has a card file of its own", row.name));
            assert_eq!(script.name, row.name);
            assert_eq!(
                script.has_keyword(Keyword::Hidden),
                row.text.starts_with("[Hidden]"),
                "{} carries Hidden exactly when it prints it first",
                row.name
            );
            assert_eq!(
                script.has_keyword(Keyword::Empower(Cost::FREE)),
                row.text.contains("[Empower]") && row.name != "Risen Altar",
                "{} carries Empower exactly when it prints the keyword",
                row.name
            );
            assert_eq!(
                script.flow_cost().is_some(),
                row.text.contains("[Flow] :rb_"),
                "{} carries Flow exactly when it prints a Flow cost",
                row.name
            );
        }
        let equipment: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind.as_deref() == Some(KIND_GEAR))
            .filter(|row| row.text.contains("[Equip]"))
            .collect();
        assert_eq!(equipment.len(), 4);
        for row in &equipment {
            let script = script_of(&row.name).unwrap();
            assert!(script.is_equipment(), "{} carries its Equip cost", row.name);
            assert!(
                row.text.ends_with(":rb_might:"),
                "{} ends with the attached might badge",
                row.name
            );
        }
        assert_eq!(
            script_of("Jagged Cutlass").unwrap().equip_cost(),
            Some(Cost {
                energy: 0,
                power: &[Power::Domain(Domain::Body)]
            })
        );
        assert_eq!(
            script_of("Shady Spectacles").unwrap().equip_cost(),
            Some(Cost {
                energy: 1,
                power: &[Power::Domain(Domain::Order)]
            })
        );
        let mut vanillas: Vec<&str> = rows
            .iter()
            .filter(|row| keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        vanillas.sort_unstable();
        assert_eq!(
            vanillas,
            ["Horns of the Dragon", "Soulspinner"],
            "two vanillas whose printed keywords are the whole script"
        );
        assert_eq!(
            script_of("Horns of the Dragon").unwrap().keywords,
            [Keyword::Tank]
        );
        assert_eq!(
            script_of("Soulspinner").unwrap().keywords,
            [Keyword::Ambush]
        );
        assert_eq!(
            script_of("Steel Paws").unwrap().keywords,
            [
                Keyword::Deflect(1),
                Keyword::Empower(Cost {
                    energy: 7,
                    power: &[]
                })
            ]
        );
        assert_eq!(
            script_of("Shen, Leader of the Kinkou Order")
                .unwrap()
                .keywords,
            [Keyword::Shield(1)],
            "a Shield without a number is Shield 1"
        );
        assert_eq!(
            script_of("Resonating Strike").unwrap().keywords,
            [Keyword::Hidden, Keyword::Reaction]
        );
        assert!(matches!(
            script_of("Public Execution").unwrap().keywords[0],
            Keyword::Flow(Cost {
                energy: 5,
                power: &[Power::Rainbow, Power::Rainbow]
            })
        ));
        assert!(matches!(
            script_of("Punching Poro").unwrap().keywords[0],
            Keyword::Empower(Cost::FREE)
        ));
        assert!(
            !script_of("Risen Altar")
                .unwrap()
                .has_keyword(Keyword::Empower(Cost::FREE)),
            "a keyword the text talks about is not the card's own"
        );
        assert!(
            !script_of("Repair Specialist")
                .unwrap()
                .has_keyword(Keyword::Assault(1)),
            "a keyword the text grants is not the card's own"
        );
        assert!(
            !script_of("Baccai Sandspinner")
                .unwrap()
                .has_keyword(Keyword::Deflect(1)),
            "a keyword an Empowered line grants is not the card's own"
        );
        for name in [
            "Shen - Eye of Twilight",
            "Renekton - Butcher of the Sands",
            "Decree of Rage",
        ] {
            let script = script_of(name).unwrap();
            assert_eq!(
                script.has_keyword(Keyword::Action) || script.has_keyword(Keyword::Reaction),
                name == "Decree of Rage",
                "{name}: an ability's timing is not the card's keyword, a spell's is"
            );
        }
        assert!(script_of("Swain, Visionary")
            .unwrap()
            .has_keyword(Keyword::Vision));
        const SEAM_STUBS: [&str; 7] = [
            "Affectionate Poro",
            "Dragon Roost",
            "Mask Mother",
            "Ol' Poro",
            "Ravenbloom Prefect",
            "Renekton - Butcher of the Sands",
            "Threshold of the Gray",
        ];
        let mut stubs: Vec<&str> = rows
            .iter()
            .filter(|row| {
                script_of(&row.name).is_some_and(|script| {
                    script.abilities.is_empty()
                        && script.statics.is_empty()
                        && script.replacement.is_none()
                        && script.additional.is_none()
                })
            })
            .filter(|row| !keyword_only(&row.text))
            .map(|row| row.name.as_str())
            .collect();
        stubs.sort_unstable();
        assert_eq!(
            stubs, SEAM_STUBS,
            "every stub with rules text is a card whose whole text waits on an engine seam"
        );
        assert_eq!(
            rows.len() - stubs.len() - vanillas.len(),
            157,
            "the other 157 carry a written script, the 11 shared with the deck files included"
        );
    }

    #[test]
    fn names_resolve_to_scripts_runes_by_suffix_and_tokens_by_name() {
        assert_eq!(script_of("Rockfall Path").unwrap().name, "Rockfall Path");
        assert!(std::ptr::eq(
            script_of("Calm Rune").unwrap(),
            &generic::RUNE
        ));
        assert!(std::ptr::eq(
            script_of("Body Rune").unwrap(),
            &generic::RUNE
        ));
        assert!(std::ptr::eq(script_of("Sprite").unwrap(), &generic::SPRITE));
        assert!(std::ptr::eq(script_of("Gold").unwrap(), &gold::CARD));
        assert!(std::ptr::eq(
            script_of("Sand Soldier").unwrap(),
            &sand_soldier::CARD
        ));
        assert!(std::ptr::eq(
            script_of("Tentacle").unwrap(),
            &tentacle::CARD
        ));
        assert!(script_of("Nobody").is_none());
        let unit = CardInfo {
            name: "Vi".into(),
            kind: Some("Unit".into()),
            ..CardInfo::default()
        };
        assert!(std::ptr::eq(resolve(&unit).unwrap(), &generic::UNIT));
        let hidden = CardInfo::default();
        assert!(resolve(&hidden).is_none());
        let unknown = CardInfo {
            name: "Mystery".into(),
            ..CardInfo::default()
        };
        assert!(std::ptr::eq(resolve(&unknown).unwrap(), &generic::UNIT));
        assert!(generic::SPRITE.has_keyword(Keyword::Temporary));
        assert!(rockfall_path::CARD.has_static(Static::NoUnitsPlayedHere));
    }

    #[test]
    fn a_print_name_resolves_to_the_base_script() {
        assert_eq!(base_name("Rockfall Path (Starter)"), "Rockfall Path");
        assert_eq!(base_name("Defy (Alternate Art)"), "Defy");
        assert_eq!(base_name("Teemo - Scout (GG EZ)"), "Teemo - Scout (GG EZ)");
        assert_eq!(base_name("Charm"), "Charm");
        assert_eq!(base_name("Charm (Foil) (Promo)"), "Charm (Foil)");
        assert_eq!(base_name("Baron Nashor (Ultimate)"), "Baron Nashor");
        let print = CardInfo {
            name: "Rockfall Path (Overnumbered)".into(),
            kind: Some("Battlefield".into()),
            ..CardInfo::default()
        };
        assert!(std::ptr::eq(resolve(&print).unwrap(), &rockfall_path::CARD));
        let hidden_print = CardInfo {
            name: "Temporal Breach (Metal)".into(),
            kind: Some("Spell".into()),
            ..CardInfo::default()
        };
        assert!(resolve(&hidden_print).unwrap().has_keyword(Keyword::Hidden));
    }

    #[test]
    fn domains_and_keywords_map_to_codes() {
        for domain in Domain::ALL {
            assert_eq!(Domain::from_code(domain.code()), Some(domain));
            assert_eq!(Domain::parse(domain.label()), Some(domain));
        }
        assert_eq!(Domain::parse("calm"), Some(Domain::Calm));
        assert_eq!(Domain::parse("Colorless"), None);
        for keyword in [
            Keyword::Accelerate,
            Keyword::Assault(2),
            Keyword::Deflect(1),
            Keyword::Ganking,
            Keyword::Weaponmaster,
            Keyword::Hunt(2),
            Keyword::Ambush,
        ] {
            let (code, arg) = keyword.codes();
            assert_eq!(Keyword::from_codes(code, arg), Some(keyword));
        }
        for keyword in [
            Keyword::Equip(Cost {
                energy: 1,
                power: &[],
            }),
            Keyword::Repeat(Cost {
                energy: 2,
                power: &[Power::Own],
            }),
            Keyword::Empower(Cost {
                energy: 8,
                power: &[],
            }),
            Keyword::Flow(Cost {
                energy: 4,
                power: &[],
            }),
        ] {
            let (code, arg) = keyword.codes();
            assert_eq!(
                Keyword::from_codes(code, arg),
                None,
                "the wire cannot carry a granted cost"
            );
        }
        assert!(Keyword::Assault(1).same_kind(Keyword::Assault(3)));
        assert!(Keyword::Hunt(1).same_kind(Keyword::Hunt(2)));
        assert!(!Keyword::Tank.same_kind(Keyword::Backline));
        assert!(Static::NoMoveToBase.same_kind(Static::NoMoveToBase));
        assert!(!Static::NoMoveToBase.same_kind(Static::AmbushIntoEnemies));
        assert!(Static::Level(6, &[]).same_kind(Static::Level(2, &[])));
        let hunter = Card {
            name: "Hunter",
            keywords: &[
                Keyword::Hunt(2),
                Keyword::Hunt(1),
                Keyword::Flow(Cost::FREE),
            ],
            abilities: &[],
            statics: &[],
            replacement: None,
            additional: None,
            names: None,
            kind: None,
            adds: None,
        };
        assert_eq!(hunter.hunt(), 3);
        assert_eq!(hunter.flow_cost(), Some(Cost::FREE));
        assert_eq!(hunter.empower_cost(), None);
        assert!(!hunter.is_equipment());
        assert_eq!(Once::default(), Once::Never);
    }

    #[test]
    fn resolved_scripts_are_keyed_by_card_id() {
        let table = Snapshot {
            cards: vec![
                CardInfo {
                    id: 9,
                    name: "Rockfall Path".into(),
                    kind: Some("Battlefield".into()),
                    ..CardInfo::default()
                },
                CardInfo {
                    id: 3,
                    name: "Vi".into(),
                    kind: Some("Unit".into()),
                    ..CardInfo::default()
                },
                CardInfo {
                    id: 5,
                    ..CardInfo::default()
                },
            ],
            ..Snapshot::default()
        };
        let resolved = Resolved::of(&table);
        assert_eq!(resolved.of_card(9).unwrap().name, "Rockfall Path");
        assert!(resolved.is_generic(3));
        assert!(!resolved.is_generic(9));
        assert!(resolved.of_card(5).is_none());
        assert!(resolved.of_card(77).is_none());
    }
}
