//! Clef and rest glyphs. The outlines are taken from the vector models in
//! TuxGuitar's `TGClefPainter` and `TGSilencePainter` (LGPL-2.1 or later).
//! egui cannot fill concave paths, so each outline is rasterized once into
//! a coverage mask and drawn as a tinted texture.

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureFilter, TextureHandle, TextureOptions, Vec2};

#[derive(Clone, Copy)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Curve(f32, f32, f32, f32, f32, f32),
}

/// Clef outlines are in staff-line gaps; rest outlines are in ninths of a
/// staff-line gap.
const CLEF_TEXELS_PER_UNIT: f32 = 14.0;
const REST_TEXELS_PER_UNIT: f32 = 3.0;
const SUBSCANLINES: usize = 4;
const CURVE_STEPS: usize = 10;

const TREBLE: &[Seg] = &[
    Seg::Move(0.9706, -0.9856),
    Seg::Curve(0.9902, -0.9954, 0.9935, -0.9954, 1.0131, -0.9954),
    Seg::Curve(1.0393, -0.9921, 1.0556, -0.9823, 1.0916, -0.9431),
    Seg::Curve(1.327, -0.7011, 1.5232, -0.2695, 1.5722, 0.0967),
    Seg::Curve(1.5788, 0.1555, 1.5788, 0.2928, 1.5722, 0.355),
    Seg::Curve(1.5363, 0.6917, 1.3957, 0.9991, 1.072, 1.4405),
    Seg::Line(1.0262, 1.5026),
    Seg::Line(1.0524, 1.5778),
    Seg::Curve(1.1145, 1.7576, 1.1864, 1.9767, 1.2322, 2.1369),
    Seg::Curve(1.2453, 2.1761, 1.2551, 2.2088, 1.2551, 2.2121),
    Seg::Curve(1.2551, 2.2121, 1.278, 2.2121, 1.3009, 2.2154),
    Seg::Curve(1.4153, 2.2252, 1.5134, 2.2513, 1.6147, 2.3004),
    Seg::Curve(1.6965, 2.3429, 1.7684, 2.3919, 1.8371, 2.4573),
    Seg::Curve(1.9123, 2.5325, 1.9613, 2.6012, 2.0071, 2.696),
    Seg::Curve(2.0758, 2.8333, 2.1052, 2.9772, 2.1019, 3.1211),
    Seg::Curve(2.0986, 3.2126, 2.0856, 3.2813, 2.0561, 3.3663),
    Seg::Curve(2.0169, 3.4905, 1.945, 3.605, 1.8469, 3.7031),
    Seg::Curve(1.7619, 3.7881, 1.6834, 3.8404, 1.5788, 3.8829),
    Seg::Line(1.5395, 3.8992),
    Seg::Line(1.5395, 4.0169),
    Seg::Curve(1.5395, 4.1837, 1.5297, 4.3799, 1.5166, 4.4976),
    Seg::Curve(1.5036, 4.6186, 1.4709, 4.733, 1.4186, 4.8376),
    Seg::Curve(1.2812, 5.1155, 1.0393, 5.2627, 0.7679, 5.2365),
    Seg::Curve(0.4606, 5.2071, 0.2121, 4.9978, 0.1499, 4.7166),
    Seg::Curve(0.1205, 4.5891, 0.1336, 4.491, 0.1859, 4.4093),
    Seg::Curve(0.2546, 4.3014, 0.3854, 4.2393, 0.5129, 4.2425),
    Seg::Curve(0.6633, 4.2491, 0.7843, 4.3668, 0.8071, 4.5303),
    Seg::Curve(0.8235, 4.6611, 0.7679, 4.782, 0.6633, 4.8474),
    Seg::Curve(0.6044, 4.8834, 0.5227, 4.8997, 0.4606, 4.8899),
    Seg::Curve(0.4508, 4.8899, 0.4409, 4.8867, 0.4409, 4.8899),
    Seg::Line(0.4802, 4.9292),
    Seg::Curve(0.5521, 5.0011, 0.6339, 5.0469, 0.7385, 5.0698),
    Seg::Curve(0.7614, 5.073, 0.781, 5.073, 0.8333, 5.073),
    Seg::Curve(0.8922, 5.073, 0.8987, 5.073, 0.9314, 5.0665),
    Seg::Curve(0.9772, 5.0534, 1.0164, 5.0371, 1.0524, 5.0174),
    Seg::Curve(1.2158, 4.9194, 1.327, 4.7166, 1.3564, 4.4714),
    Seg::Curve(1.3662, 4.3799, 1.3761, 4.1837, 1.3761, 4.0398),
    Seg::Curve(1.3761, 3.945, 1.3761, 3.9385, 1.3695, 3.9385),
    Seg::Curve(1.3564, 3.945, 1.2878, 3.9516, 1.2387, 3.9548),
    Seg::Curve(1.1766, 3.9581, 1.072, 3.9581, 1.0229, 3.9516),
    Seg::Curve(0.83, 3.9287, 0.6535, 3.8633, 0.4998, 3.7554),
    Seg::Curve(0.2709, 3.5952, 0.1074, 3.3565, 0.0388, 3.0818),
    Seg::Curve(-0.0495, 2.7385, 0.0126, 2.3723, 0.2284, 1.9865),
    Seg::Curve(0.32, 1.8263, 0.4082, 1.6955, 0.6273, 1.4045),
    Seg::Line(0.7516, 1.2378),
    Seg::Line(0.7287, 1.1756),
    Seg::Curve(0.5717, 0.7604, 0.5194, 0.5217, 0.5031, 0.1751),
    Seg::Curve(0.4965, 0.0247, 0.5031, -0.1028, 0.5227, -0.2074),
    Seg::Curve(0.5717, -0.4821, 0.6992, -0.7273, 0.8987, -0.9235),
    Seg::Curve(0.9379, -0.9627, 0.9543, -0.9758, 0.9706, -0.9856),
    Seg::Move(1.2289, -0.4265),
    Seg::Curve(1.1962, -0.4821, 1.2028, -0.4788, 1.1733, -0.4592),
    Seg::Curve(1.0818, -0.4036, 0.987, -0.3186, 0.9183, -0.2336),
    Seg::Curve(0.7777, -0.0537, 0.6992, 0.1555, 0.6894, 0.3811),
    Seg::Curve(0.6862, 0.4727, 0.696, 0.5315, 0.7385, 0.6721),
    Seg::Curve(0.7712, 0.7866, 0.8627, 1.0645, 0.8693, 1.071),
    Seg::Curve(0.8693, 1.0743, 0.8889, 1.0514, 0.9085, 1.022),
    Seg::Curve(1.1341, 0.6983, 1.2551, 0.4334, 1.3009, 0.1686),
    Seg::Curve(1.3107, 0.0999, 1.3172, 0.0444, 1.3205, -0.0341),
    Seg::Curve(1.3237, -0.2139, 1.3041, -0.3022, 1.2289, -0.4265),
    Seg::Move(0.9477, 1.774),
    Seg::Curve(0.9281, 1.7119, 0.9085, 1.6628, 0.9085, 1.6628),
    Seg::Curve(0.9052, 1.6628, 0.7679, 1.8459, 0.7091, 1.9277),
    Seg::Curve(0.5161, 2.199, 0.3984, 2.4181, 0.3298, 2.6339),
    Seg::Curve(0.284, 2.7679, 0.2644, 2.8922, 0.2578, 3.0295),
    Seg::Curve(0.2578, 3.1047, 0.2611, 3.1505, 0.2742, 3.2061),
    Seg::Curve(0.3494, 3.4971, 0.66, 3.7358, 1.0262, 3.7881),
    Seg::Curve(1.0818, 3.7946, 1.2387, 3.7946, 1.3074, 3.7881),
    Seg::Curve(1.3761, 3.7783, 1.3695, 3.7815, 1.3662, 3.7521),
    Seg::Curve(1.3466, 3.4415, 1.3205, 3.2519, 1.2682, 2.9707),
    Seg::Curve(1.2485, 2.853, 1.193, 2.6045, 1.1897, 2.6012),
    Seg::Curve(1.1864, 2.5979, 1.1243, 2.6045, 1.0883, 2.6143),
    Seg::Curve(1.0426, 2.6241, 1.0131, 2.6339, 0.9739, 2.6535),
    Seg::Curve(0.7614, 2.7614, 0.6992, 3.0066, 0.8333, 3.2061),
    Seg::Curve(0.8562, 3.2388, 0.915, 3.2976, 0.951, 3.3205),
    Seg::Curve(0.9641, 3.3303, 0.9935, 3.3499, 1.0131, 3.3597),
    Seg::Curve(1.0426, 3.3761, 1.0491, 3.3826, 1.0556, 3.3957),
    Seg::Curve(1.0818, 3.4382, 1.0654, 3.4905, 1.0164, 3.5134),
    Seg::Curve(0.9902, 3.5265, 0.9706, 3.5232, 0.9183, 3.4971),
    Seg::Curve(0.7908, 3.4349, 0.696, 3.3499, 0.6273, 3.2453),
    Seg::Curve(0.4998, 3.0491, 0.4834, 2.7974, 0.5881, 2.5848),
    Seg::Curve(0.6796, 2.4017, 0.8431, 2.2775, 1.0524, 2.2317),
    Seg::Curve(1.072, 2.2252, 1.0883, 2.2219, 1.0883, 2.2219),
    Seg::Curve(1.0916, 2.2187, 1.0, 1.9211, 0.9477, 1.774),
    Seg::Move(1.3924, 2.6175),
    Seg::Curve(1.3826, 2.6175, 1.3728, 2.6143, 1.3695, 2.6143),
    Seg::Line(1.3597, 2.611),
    Seg::Line(1.3761, 2.6797),
    Seg::Curve(1.4513, 3.0099, 1.5068, 3.3696, 1.5265, 3.6736),
    Seg::Curve(1.5297, 3.7063, 1.5297, 3.7325, 1.5297, 3.7325),
    Seg::Curve(1.533, 3.7325, 1.5755, 3.7129, 1.5984, 3.6965),
    Seg::Curve(1.654, 3.6638, 1.7063, 3.6148, 1.7488, 3.5592),
    Seg::Curve(1.8501, 3.4251, 1.8861, 3.2584, 1.8469, 3.0884),
    Seg::Curve(1.8175, 2.951, 1.739, 2.8235, 1.6278, 2.7353),
    Seg::Curve(1.5592, 2.6829, 1.4676, 2.6372, 1.3924, 2.6175),
];

const BASS: &[Seg] = &[
    Seg::Move(0.7194, 0.1615),
    Seg::Curve(0.7545, 0.1583, 0.892, 0.1615, 0.9432, 0.1647),
    Seg::Curve(1.3429, 0.2094, 1.6147, 0.4077, 1.7043, 0.7211),
    Seg::Curve(1.7267, 0.8074, 1.7363, 0.8714, 1.7331, 0.9801),
    Seg::Curve(1.7331, 1.1176, 1.7171, 1.2327, 1.6723, 1.3542),
    Seg::Curve(1.5092, 1.8371, 1.0328, 2.2368, 0.163, 2.6205),
    Seg::Curve(0.1278, 2.6365, 0.0958, 2.6525, 0.0926, 2.6557),
    Seg::Curve(0.0734, 2.6685, 0.0542, 2.6717, 0.0351, 2.6621),
    Seg::Curve(0.0191, 2.6557, 0.0127, 2.6493, 0.0063, 2.6333),
    Seg::Curve(-0.0033, 2.6173, -0.0001, 2.6014, 0.0063, 2.5886),
    Seg::Curve(0.0127, 2.5758, 0.0255, 2.5662, 0.0958, 2.5246),
    Seg::Curve(0.3804, 2.3551, 0.5883, 2.2048, 0.7641, 2.0513),
    Seg::Curve(0.8185, 2.0002, 0.9208, 1.8979, 0.9656, 1.8467),
    Seg::Curve(1.1607, 1.6197, 1.263, 1.399, 1.2982, 1.1432),
    Seg::Curve(1.3046, 1.0824, 1.3046, 0.9385, 1.2982, 0.8778),
    Seg::Curve(1.2854, 0.785, 1.263, 0.6987, 1.2342, 0.622),
    Seg::Curve(1.2118, 0.5708, 1.2054, 0.5548, 1.1798, 0.5132),
    Seg::Curve(1.0871, 0.3565, 0.9464, 0.2702, 0.7897, 0.2702),
    Seg::Curve(0.6362, 0.2702, 0.5083, 0.3565, 0.4252, 0.5196),
    Seg::Curve(0.4124, 0.5452, 0.3804, 0.6283, 0.3804, 0.6315),
    Seg::Curve(0.3804, 0.6347, 0.3932, 0.6315, 0.406, 0.6315),
    Seg::Curve(0.4795, 0.6283, 0.5595, 0.6571, 0.6202, 0.7051),
    Seg::Curve(0.7354, 0.8042, 0.7673, 0.9641, 0.697, 1.108),
    Seg::Curve(0.649, 1.2135, 0.5467, 1.2807, 0.4348, 1.2903),
    Seg::Curve(0.2781, 1.2999, 0.1246, 1.1975, 0.0734, 1.0504),
    Seg::Curve(0.0574, 1.0025, 0.0542, 0.9545, 0.0606, 0.8778),
    Seg::Curve(0.0894, 0.558, 0.2941, 0.2862, 0.5755, 0.1935),
    Seg::Curve(0.6234, 0.1775, 0.6714, 0.1679, 0.7194, 0.1615),
    Seg::Move(1.9633, 0.4621),
    Seg::Curve(2.0145, 0.4461, 2.0752, 0.4621, 2.1168, 0.5004),
    Seg::Curve(2.1456, 0.5324, 2.1616, 0.574, 2.1616, 0.6156),
    Seg::Curve(2.1616, 0.7019, 2.0912, 0.7722, 2.0049, 0.7722),
    Seg::Curve(1.9153, 0.7722, 1.845, 0.7019, 1.845, 0.6156),
    Seg::Curve(1.845, 0.542, 1.8929, 0.4812, 1.9633, 0.4621),
    Seg::Move(1.9633, 1.2583),
    Seg::Curve(2.0145, 1.2423, 2.0752, 1.2583, 2.1168, 1.2967),
    Seg::Curve(2.1552, 1.3382, 2.1712, 1.3958, 2.1552, 1.447),
    Seg::Curve(2.1392, 1.5205, 2.0784, 1.5685, 2.0049, 1.5685),
    Seg::Curve(1.9153, 1.5685, 1.845, 1.4981, 1.845, 1.4118),
    Seg::Curve(1.845, 1.3382, 1.8929, 1.2775, 1.9633, 1.2583),
];

const QUARTER: &[Seg] = &[
    Seg::Move(2.1034, 0.0473),
    Seg::Curve(2.1979, 0.0, 2.2925, 0.0, 2.387, 0.0236),
    Seg::Curve(2.4579, 0.0709, 6.1212, 4.4195, 6.2157, 4.6086),
    Seg::Curve(6.3811, 4.9158, 6.3339, 5.2231, 6.0975, 5.6012),
    Seg::Curve(5.9085, 5.8612, 5.5303, 6.2393, 4.8213, 6.8302),
    Seg::Curve(4.4668, 7.1138, 4.1359, 7.421, 4.065, 7.4919),
    Seg::Curve(3.3324, 8.2718, 3.1669, 9.4535, 3.5923, 10.3516),
    Seg::Curve(3.7342, 10.6588, 3.805, 10.7297, 4.6322, 11.6751),
    Seg::Curve(6.1448, 13.4476, 6.0739, 13.3767, 6.0739, 13.5185),
    Seg::Curve(6.0739, 13.6603, 5.9321, 13.8021, 5.7903, 13.8258),
    Seg::Curve(5.6721, 13.8258, 5.5776, 13.7785, 5.4121, 13.613),
    Seg::Curve(4.8213, 13.0222, 3.4742, 12.5968, 2.7415, 12.7622),
    Seg::Curve(2.387, 12.8331, 2.1979, 12.9749, 2.0325, 13.3058),
    Seg::Curve(1.938, 13.5421, 1.8671, 13.8258, 1.8434, 14.2039),
    Seg::Curve(1.8198, 14.7238, 1.9144, 15.2674, 2.0798, 15.7874),
    Seg::Curve(2.1979, 16.1182, 2.2689, 16.3073, 2.4343, 16.5436),
    Seg::Curve(2.5761, 16.7327, 2.5761, 16.8036, 2.4579, 16.8745),
    Seg::Curve(2.3634, 16.9218, 2.3161, 16.8982, 2.1034, 16.6382),
    Seg::Curve(1.2526, 15.551, 0.4727, 13.9912, 0.2127, 12.8331),
    Seg::Curve(0.0, 12.006, 0.0945, 11.4387, 0.4254, 11.2024),
    Seg::Curve(0.5908, 11.1079, 0.7563, 11.0842, 1.1108, 11.0842),
    Seg::Curve(1.7962, 11.1315, 2.7652, 11.3915, 3.7105, 11.7932),
    Seg::Line(4.0178, 11.9351),
    Seg::Line(2.7179, 10.3989),
    Seg::Curve(0.7327, 8.0591, 0.6617, 7.9646, 0.5908, 7.8228),
    Seg::Curve(0.52, 7.6337, 0.4963, 7.4446, 0.5672, 7.2319),
    Seg::Curve(0.6854, 6.8538, 1.0163, 6.4757, 2.0089, 5.6485),
    Seg::Curve(2.3634, 5.3649, 2.6706, 5.0813, 2.7415, 5.034),
    Seg::Curve(3.2851, 4.4431, 3.5451, 3.616, 3.4505, 2.8361),
    Seg::Curve(3.4033, 2.5761, 3.2851, 2.1743, 3.1433, 1.9616),
    Seg::Curve(3.0724, 1.8434, 2.7888, 1.4889, 2.5052, 1.1344),
    Seg::Curve(2.2216, 0.8035, 1.9616, 0.4963, 1.938, 0.449),
    Seg::Curve(1.8671, 0.3072, 1.938, 0.1182, 2.1034, 0.0473),
];

const EIGHTH: &[Seg] = &[
    Seg::Move(1.678, 0.0709),
    Seg::Curve(2.1979, 0.0, 2.647, 0.1418, 3.0015, 0.4727),
    Seg::Curve(3.2142, 0.709, 3.3087, 0.8981, 3.4505, 1.4653),
    Seg::Curve(3.5214, 1.7253, 3.5923, 1.9852, 3.616, 2.0325),
    Seg::Curve(3.7105, 2.2452, 3.8759, 2.387, 4.0886, 2.4106),
    Seg::Curve(4.1832, 2.4106, 4.2304, 2.4106, 4.3486, 2.3397),
    Seg::Curve(4.6559, 2.1979, 5.3885, 1.3235, 5.6721, 0.7799),
    Seg::Curve(5.743, 0.6145, 5.8612, 0.5436, 5.9793, 0.5436),
    Seg::Curve(6.0739, 0.5436, 6.192, 0.6145, 6.2393, 0.6854),
    Seg::Curve(6.263, 0.7326, 6.0266, 1.5598, 4.774, 5.743),
    Seg::Curve(3.9705, 8.4845, 3.2851, 10.777, 3.2615, 10.8006),
    Seg::Curve(3.2615, 10.8243, 3.1669, 10.8952, 3.0724, 10.9188),
    Seg::Curve(2.9306, 10.9897, 2.907, 10.9897, 2.7179, 10.9897),
    Seg::Curve(2.5288, 10.9897, 2.4815, 10.9897, 2.3397, 10.9424),
    Seg::Curve(2.2688, 10.8952, 2.1743, 10.8479, 2.1743, 10.8243),
    Seg::Curve(2.127, 10.8006, 2.2925, 10.3752, 3.4978, 6.9483),
    Seg::Curve(4.2541, 4.8449, 4.8686, 3.096, 4.8449, 3.096),
    Seg::Line(4.4431, 3.2142),
    Seg::Curve(3.5923, 3.4978, 3.0724, 3.5923, 2.4343, 3.5923),
    Seg::Curve(1.9143, 3.5923, 1.7725, 3.5687, 1.3944, 3.3796),
    Seg::Curve(0.3545, 2.8833, 0.0, 1.678, 0.5908, 0.7799),
    Seg::Curve(0.8508, 0.4254, 1.2526, 0.1418, 1.678, 0.0709),
];

const SIXTEENTH: &[Seg] = &[
    Seg::Move(3.5214, 0.0709),
    Seg::Curve(4.0414, 0.0, 4.4904, 0.1418, 4.8449, 0.4727),
    Seg::Curve(5.0576, 0.709, 5.1522, 0.8981, 5.294, 1.4653),
    Seg::Curve(5.3649, 1.7253, 5.4358, 1.9852, 5.4594, 2.0325),
    Seg::Curve(5.5776, 2.3397, 5.9084, 2.4815, 6.1448, 2.3634),
    Seg::Curve(6.452, 2.2216, 7.0901, 1.3944, 7.421, 0.7326),
    Seg::Curve(7.4683, 0.6145, 7.6101, 0.5436, 7.7046, 0.5436),
    Seg::Curve(7.7992, 0.5436, 7.9173, 0.6145, 7.9646, 0.6854),
    Seg::Curve(7.9882, 0.7326, 7.6101, 2.1507, 5.8612, 8.6972),
    Seg::Curve(4.6795, 13.0695, 3.7105, 16.6854, 3.6869, 16.7091),
    Seg::Curve(3.6632, 16.7327, 3.5923, 16.8036, 3.4978, 16.8272),
    Seg::Curve(3.356, 16.8982, 3.3323, 16.8982, 3.1433, 16.8982),
    Seg::Curve(2.9542, 16.8982, 2.9069, 16.8982, 2.7652, 16.8509),
    Seg::Curve(2.6942, 16.8036, 2.5997, 16.7563, 2.5997, 16.7327),
    Seg::Curve(2.5524, 16.7091, 2.6942, 16.2837, 3.7341, 12.8568),
    Seg::Curve(4.3722, 10.7534, 4.8922, 9.0281, 4.8922, 9.0045),
    Seg::Curve(4.8686, 9.0045, 4.6795, 9.0517, 4.4668, 9.1226),
    Seg::Curve(3.616, 9.4062, 3.0724, 9.5008, 2.4343, 9.5008),
    Seg::Curve(1.9143, 9.5008, 1.7725, 9.4772, 1.3944, 9.2881),
    Seg::Curve(0.3545, 8.7918, 0.0, 7.5864, 0.5908, 6.6884),
    Seg::Curve(1.158, 5.8612, 2.3397, 5.7194, 3.0015, 6.3575),
    Seg::Curve(3.2378, 6.6175, 3.3323, 6.8302, 3.4742, 7.4446),
    Seg::Curve(3.5451, 7.7991, 3.6396, 8.0591, 3.7341, 8.1536),
    Seg::Curve(3.805, 8.2246, 3.9468, 8.2955, 4.0886, 8.3191),
    Seg::Curve(4.1832, 8.3191, 4.2305, 8.3191, 4.3486, 8.2482),
    Seg::Curve(4.6086, 8.13, 5.1522, 7.5155, 5.4594, 6.9956),
    Seg::Curve(5.5539, 6.8302, 5.6012, 6.6884, 5.7666, 6.1448),
    Seg::Curve(6.2866, 4.4904, 6.6884, 3.1197, 6.6884, 3.096),
    Seg::Curve(6.6884, 3.096, 6.5229, 3.1433, 6.3575, 3.2142),
    Seg::Curve(5.9557, 3.3324, 5.4121, 3.4742, 5.034, 3.5451),
    Seg::Curve(4.774, 3.5923, 4.6322, 3.5923, 4.2777, 3.5923),
    Seg::Curve(3.7578, 3.5923, 3.616, 3.5687, 3.2378, 3.3796),
    Seg::Curve(2.1979, 2.8833, 1.8434, 1.678, 2.4343, 0.7799),
    Seg::Curve(2.6942, 0.4254, 3.096, 0.1418, 3.5214, 0.0709),
];

const THIRTY_SECOND: &[Seg] = &[
    Seg::Move(4.9395, 0.0709),
    Seg::Curve(5.4594, 0.0, 5.9085, 0.1418, 6.263, 0.4727),
    Seg::Curve(6.4757, 0.709, 6.5702, 0.8981, 6.712, 1.4653),
    Seg::Curve(6.7829, 1.7253, 6.8538, 1.9852, 6.8774, 2.0325),
    Seg::Curve(6.972, 2.2452, 7.1374, 2.387, 7.3501, 2.4106),
    Seg::Curve(7.4446, 2.4106, 7.4919, 2.4106, 7.5628, 2.3634),
    Seg::Curve(7.8228, 2.1979, 8.3427, 1.4889, 8.6972, 0.8035),
    Seg::Curve(8.7681, 0.6145, 8.8627, 0.5436, 9.0045, 0.5436),
    Seg::Curve(9.099, 0.5436, 9.2172, 0.6145, 9.2644, 0.6854),
    Seg::Curve(9.2881, 0.7326, 8.8154, 2.7888, 6.7356, 11.6278),
    Seg::Curve(5.3176, 17.6308, 4.1359, 22.5703, 4.1359, 22.5939),
    Seg::Curve(4.1123, 22.6412, 4.0414, 22.6884, 3.9468, 22.7357),
    Seg::Curve(3.7814, 22.8066, 3.7578, 22.8066, 3.5687, 22.8066),
    Seg::Curve(3.3796, 22.8066, 3.3324, 22.8066, 3.1906, 22.7593),
    Seg::Curve(3.1197, 22.7121, 3.0251, 22.6648, 3.0251, 22.6412),
    Seg::Curve(2.9779, 22.6175, 3.096, 22.1921, 3.9941, 18.7652),
    Seg::Curve(4.5377, 16.6618, 4.9867, 14.9365, 5.0104, 14.9129),
    Seg::Curve(5.0104, 14.8893, 4.9158, 14.8893, 4.6322, 15.0074),
    Seg::Curve(3.7105, 15.3147, 2.8833, 15.4565, 2.2689, 15.4329),
    Seg::Curve(1.938, 15.4092, 1.678, 15.3383, 1.3944, 15.1965),
    Seg::Curve(0.3545, 14.7002, 0.0, 13.4949, 0.5909, 12.5968),
    Seg::Curve(1.1581, 11.7696, 2.3398, 11.6278, 3.0015, 12.2659),
    Seg::Curve(3.2378, 12.5259, 3.3324, 12.7386, 3.4742, 13.3531),
    Seg::Curve(3.5451, 13.7076, 3.6396, 13.9676, 3.7342, 14.0621),
    Seg::Curve(3.8287, 14.133, 3.9705, 14.2039, 4.1123, 14.2275),
    Seg::Curve(4.2068, 14.2275, 4.2541, 14.2275, 4.3486, 14.1566),
    Seg::Curve(4.6795, 13.9912, 5.3649, 13.1404, 5.5776, 12.6677),
    Seg::Curve(5.6012, 12.5732, 6.5466, 9.0517, 6.5466, 9.0045),
    Seg::Curve(6.5466, 8.9808, 6.4048, 9.0281, 6.2393, 9.099),
    Seg::Curve(5.743, 9.2408, 5.294, 9.3826, 4.8686, 9.4535),
    Seg::Curve(4.5613, 9.5008, 4.4432, 9.5008, 4.065, 9.5008),
    Seg::Curve(3.5451, 9.5008, 3.4033, 9.4772, 3.0251, 9.2881),
    Seg::Curve(1.9852, 8.7918, 1.6307, 7.5864, 2.2216, 6.6884),
    Seg::Curve(2.7888, 5.8612, 3.9705, 5.7194, 4.6322, 6.3575),
    Seg::Curve(4.8686, 6.6175, 4.9631, 6.8302, 5.1049, 7.4446),
    Seg::Curve(5.1758, 7.7991, 5.2703, 8.0591, 5.3649, 8.1536),
    Seg::Curve(5.5067, 8.2955, 5.7666, 8.3664, 5.9321, 8.2718),
    Seg::Curve(6.1684, 8.1773, 6.4757, 7.8228, 6.7829, 7.3501),
    Seg::Curve(7.1138, 6.8774, 7.0901, 6.9483, 7.6101, 4.9631),
    Seg::Curve(7.8701, 3.9705, 8.0828, 3.1433, 8.0828, 3.1197),
    Seg::Curve(8.0828, 3.1197, 7.9173, 3.1433, 7.7283, 3.2142),
    Seg::Curve(7.2792, 3.356, 6.8302, 3.4742, 6.452, 3.5451),
    Seg::Curve(6.1921, 3.5923, 6.0502, 3.5923, 5.6957, 3.5923),
    Seg::Curve(5.1758, 3.5923, 5.034, 3.5687, 4.6559, 3.3796),
    Seg::Curve(3.616, 2.8833, 3.2615, 1.678, 3.8523, 0.7799),
    Seg::Curve(4.1123, 0.4254, 4.5141, 0.1418, 4.9395, 0.0709),
];

const SIXTY_FOURTH: &[Seg] = &[
    Seg::Move(5.9084, 0.0709),
    Seg::Curve(6.4284, 0.0, 6.8774, 0.1418, 7.2319, 0.4727),
    Seg::Curve(7.4446, 0.709, 7.5392, 0.8981, 7.681, 1.4653),
    Seg::Curve(7.7991, 1.9616, 7.8464, 2.1034, 7.9409, 2.2216),
    Seg::Curve(8.0827, 2.387, 8.3663, 2.4579, 8.5318, 2.3634),
    Seg::Curve(8.7208, 2.2688, 9.0281, 1.8434, 9.3826, 1.1344),
    Seg::Curve(9.6426, 0.6145, 9.6898, 0.5672, 9.8553, 0.5672),
    Seg::Curve(9.9498, 0.5672, 10.068, 0.6381, 10.1152, 0.709),
    Seg::Curve(10.1389, 0.7563, 9.5717, 3.5923, 7.3265, 14.6057),
    Seg::Curve(5.7903, 22.2157, 4.514, 28.4787, 4.514, 28.5023),
    Seg::Curve(4.4904, 28.5496, 4.4195, 28.5969, 4.325, 28.6441),
    Seg::Curve(4.1595, 28.715, 4.1359, 28.715, 3.9468, 28.715),
    Seg::Curve(3.7578, 28.715, 3.7105, 28.715, 3.5687, 28.6678),
    Seg::Curve(3.4978, 28.6205, 3.4032, 28.5732, 3.4032, 28.5496),
    Seg::Curve(3.356, 28.526, 3.4505, 28.1006, 4.2304, 24.65),
    Seg::Curve(4.7031, 22.523, 5.0813, 20.7977, 5.0813, 20.7741),
    Seg::Curve(5.0813, 20.7741, 4.8922, 20.8214, 4.6795, 20.8922),
    Seg::Curve(4.1595, 21.0813, 3.7341, 21.1759, 3.2851, 21.2704),
    Seg::Curve(2.9542, 21.3177, 2.8361, 21.3177, 2.4579, 21.3177),
    Seg::Curve(1.9143, 21.3177, 1.7725, 21.294, 1.3944, 21.105),
    Seg::Curve(0.3545, 20.6087, 0.0, 19.4033, 0.5908, 18.5052),
    Seg::Curve(1.158, 17.6781, 2.3397, 17.5363, 3.0015, 18.1744),
    Seg::Curve(3.2378, 18.4343, 3.3324, 18.647, 3.4742, 19.2615),
    Seg::Curve(3.5451, 19.616, 3.6396, 19.876, 3.7341, 19.9705),
    Seg::Curve(3.805, 20.0414, 3.9468, 20.1123, 4.0886, 20.136),
    Seg::Curve(4.2541, 20.1596, 4.3959, 20.0651, 4.6559, 19.8051),
    Seg::Curve(5.0104, 19.4506, 5.4594, 18.8125, 5.5776, 18.5052),
    Seg::Curve(5.6012, 18.4343, 5.7903, 17.6072, 6.003, 16.6618),
    Seg::Curve(6.2157, 15.7165, 6.3811, 14.9366, 6.4048, 14.9129),
    Seg::Curve(6.4048, 14.8893, 6.3338, 14.8893, 6.0266, 15.0075),
    Seg::Curve(5.1049, 15.3147, 4.2777, 15.4565, 3.6632, 15.4329),
    Seg::Curve(3.3324, 15.4092, 3.0724, 15.3383, 2.7888, 15.1965),
    Seg::Curve(1.7489, 14.7002, 1.3944, 13.4949, 1.9852, 12.5968),
    Seg::Curve(2.2688, 12.1714, 2.7415, 11.9114, 3.2851, 11.8642),
    Seg::Curve(3.7105, 11.8405, 4.0886, 11.9823, 4.3959, 12.2896),
    Seg::Curve(4.6086, 12.5259, 4.7031, 12.715, 4.8449, 13.2822),
    Seg::Curve(4.9158, 13.5422, 4.9867, 13.8021, 5.0104, 13.8494),
    Seg::Curve(5.1285, 14.1566, 5.4594, 14.2984, 5.7194, 14.1803),
    Seg::Curve(6.003, 14.0385, 6.6647, 13.1877, 6.8774, 12.6913),
    Seg::Curve(6.9011, 12.6204, 7.1138, 11.746, 7.3265, 10.777),
    Seg::Line(7.7282, 8.9808),
    Seg::Line(7.3028, 9.1226),
    Seg::Curve(6.4284, 9.4062, 5.8848, 9.5008, 5.2467, 9.5008),
    Seg::Curve(4.7267, 9.5008, 4.5849, 9.4771, 4.2068, 9.2881),
    Seg::Curve(3.1669, 8.7918, 2.8124, 7.5864, 3.4032, 6.6884),
    Seg::Curve(3.6869, 6.263, 4.1595, 6.003, 4.7031, 5.9557),
    Seg::Curve(5.1285, 5.9321, 5.5067, 6.0739, 5.8139, 6.3811),
    Seg::Curve(6.0266, 6.6175, 6.1211, 6.8065, 6.2629, 7.3737),
    Seg::Curve(6.3811, 7.87, 6.4284, 8.0118, 6.5229, 8.13),
    Seg::Curve(6.6883, 8.2955, 6.9483, 8.3664, 7.1138, 8.2718),
    Seg::Curve(7.3974, 8.13, 7.8464, 7.5392, 8.13, 6.9483),
    Seg::Curve(8.2246, 6.7593, 8.2718, 6.5702, 8.6263, 4.9395),
    Seg::Line(9.0281, 3.1197),
    Seg::Curve(9.0281, 3.096, 8.8863, 3.1433, 8.7208, 3.2142),
    Seg::Curve(7.8228, 3.4978, 7.0665, 3.6396, 6.4993, 3.616),
    Seg::Curve(6.1684, 3.5923, 5.9084, 3.5214, 5.6248, 3.3796),
    Seg::Curve(4.5849, 2.8833, 4.2304, 1.678, 4.8213, 0.7799),
    Seg::Curve(5.0813, 0.4254, 5.483, 0.1418, 5.9084, 0.0709),
];

/// One rasterized outline. `min` and `size` are in the outline's own units.
pub struct Glyph {
    texture: TextureHandle,
    min: Vec2,
    size: Vec2,
}

impl Glyph {
    fn new(ctx: &egui::Context, name: &str, segs: &[Seg], texels_per_unit: f32) -> Self {
        let (image, min, size) = rasterize(segs, texels_per_unit);
        let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
        Self {
            texture: ctx.load_texture(name, image, options),
            min,
            size,
        }
    }

    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Draws the glyph with its outline origin at `origin`, one outline unit
    /// being `scale` points.
    pub fn paint(&self, painter: &egui::Painter, origin: Pos2, scale: f32, tint: Color32) {
        let rect = Rect::from_min_size(origin + self.min * scale, self.size * scale);
        painter.image(
            self.texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            tint,
        );
    }
}

pub struct Symbols {
    pub treble_clef: Glyph,
    pub bass_clef: Glyph,
    quarter_rest: Glyph,
    eighth_rest: Glyph,
    sixteenth_rest: Glyph,
    thirty_second_rest: Glyph,
    sixty_fourth_rest: Glyph,
}

impl Symbols {
    pub fn new(ctx: &egui::Context) -> Self {
        let clef = |name, segs| Glyph::new(ctx, name, segs, CLEF_TEXELS_PER_UNIT);
        let rest = |name, segs| Glyph::new(ctx, name, segs, REST_TEXELS_PER_UNIT);
        Self {
            treble_clef: clef("treble_clef", TREBLE),
            bass_clef: clef("bass_clef", BASS),
            quarter_rest: rest("quarter_rest", QUARTER),
            eighth_rest: rest("eighth_rest", EIGHTH),
            sixteenth_rest: rest("sixteenth_rest", SIXTEENTH),
            thirty_second_rest: rest("thirty_second_rest", THIRTY_SECOND),
            sixty_fourth_rest: rest("sixty_fourth_rest", SIXTY_FOURTH),
        }
    }

    /// The rest outline for a note value of a quarter or shorter, and the
    /// height in outline units that the glyph is centered by. Values
    /// shorter than a 64th share the 64th rest.
    pub fn rest(&self, value: u32) -> (&Glyph, f32) {
        match value {
            4 => (&self.quarter_rest, 16.0),
            8 => (&self.eighth_rest, 12.0),
            16 => (&self.sixteenth_rest, 16.0),
            32 => (&self.thirty_second_rest, 24.0),
            _ => (&self.sixty_fourth_rest, 28.0),
        }
    }
}

fn flatten(segs: &[Seg]) -> Vec<Vec<(f32, f32)>> {
    let mut paths: Vec<Vec<(f32, f32)>> = Vec::new();
    for seg in segs {
        match *seg {
            Seg::Move(x, y) => paths.push(vec![(x, y)]),
            Seg::Line(x, y) => {
                if let Some(path) = paths.last_mut() {
                    path.push((x, y));
                }
            }
            Seg::Curve(x1, y1, x2, y2, x, y) => {
                if let Some(path) = paths.last_mut() {
                    let (x0, y0) = path.last().copied().unwrap_or((x, y));
                    for step in 1..=CURVE_STEPS {
                        let t = step as f32 / CURVE_STEPS as f32;
                        let u = 1.0 - t;
                        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        path.push((
                            a * x0 + b * x1 + c * x2 + d * x,
                            a * y0 + b * y1 + c * y2 + d * y,
                        ));
                    }
                }
            }
        }
    }
    paths
}

/// Fills the outline with the nonzero rule using horizontal sub-scanlines
/// and exact horizontal span coverage. Returns the mask and the outline-unit
/// rectangle it covers.
fn rasterize(segs: &[Seg], texels_per_unit: f32) -> (ColorImage, Vec2, Vec2) {
    let paths = flatten(segs);
    let points = paths.iter().flatten();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let pad = 1.0 / texels_per_unit;
    let (origin_x, origin_y) = (min_x - pad, min_y - pad);
    let width = ((max_x - min_x + 2.0 * pad) * texels_per_unit).ceil() as usize;
    let height = ((max_y - min_y + 2.0 * pad) * texels_per_unit).ceil() as usize;

    // (x0, y0, x1, y1, winding) in texel coordinates, oriented downwards.
    let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
    for path in &paths {
        for index in 0..path.len() {
            let (ax, ay) = path[index];
            let (bx, by) = path[(index + 1) % path.len()];
            let a = ((ax - origin_x) * texels_per_unit, (ay - origin_y) * texels_per_unit);
            let b = ((bx - origin_x) * texels_per_unit, (by - origin_y) * texels_per_unit);
            if a.1 == b.1 {
                continue;
            }
            if a.1 < b.1 {
                edges.push((a.0, a.1, b.0, b.1, 1));
            } else {
                edges.push((b.0, b.1, a.0, a.1, -1));
            }
        }
    }

    let mut pixels = vec![Color32::TRANSPARENT; width * height];
    let mut cover = vec![0.0_f32; width];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..height {
        cover.iter_mut().for_each(|value| *value = 0.0);
        for sub in 0..SUBSCANLINES {
            let y = row as f32 + (sub as f32 + 0.5) / SUBSCANLINES as f32;
            crossings.clear();
            for &(x0, y0, x1, y1, winding) in &edges {
                if y >= y0 && y < y1 {
                    crossings.push((x0 + (y - y0) * (x1 - x0) / (y1 - y0), winding));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    add_span(&mut cover, pair[0].0, pair[1].0, 1.0 / SUBSCANLINES as f32);
                }
            }
        }
        for (column, value) in cover.iter().enumerate() {
            let alpha = (value.min(1.0) * 255.0).round() as u8;
            pixels[row * width + column] = Color32::from_white_alpha(alpha);
        }
    }
    (
        ColorImage::new([width, height], pixels),
        Vec2::new(origin_x, origin_y),
        Vec2::new(width as f32 / texels_per_unit, height as f32 / texels_per_unit),
    )
}

/// Adds `weight` times the covered length of `[from, to)` to each texel.
fn add_span(cover: &mut [f32], from: f32, to: f32, weight: f32) {
    let from = from.clamp(0.0, cover.len() as f32);
    let to = to.clamp(0.0, cover.len() as f32);
    if to <= from {
        return;
    }
    let first = from.floor() as usize;
    let last = (to.floor() as usize).min(cover.len() - 1);
    if first == last {
        cover[first] += (to - from) * weight;
        return;
    }
    cover[first] += (first as f32 + 1.0 - from) * weight;
    for value in &mut cover[first + 1..last] {
        *value += weight;
    }
    cover[last] += (to - last as f32) * weight;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(segs: &[Seg], texels_per_unit: f32) -> f32 {
        let (image, _, _) = rasterize(segs, texels_per_unit);
        image.pixels.iter().map(|pixel| pixel.a() as f32 / 255.0).sum::<f32>() / (texels_per_unit * texels_per_unit)
    }

    #[test]
    fn a_square_covers_its_area() {
        let square = [
            Seg::Move(0.0, 0.0),
            Seg::Line(2.0, 0.0),
            Seg::Line(2.0, 2.0),
            Seg::Line(0.0, 2.0),
        ];
        assert!((coverage(&square, 8.0) - 4.0).abs() < 0.05);
    }

    #[test]
    fn an_opposite_winding_inner_outline_is_a_hole() {
        let ring = [
            Seg::Move(0.0, 0.0),
            Seg::Line(4.0, 0.0),
            Seg::Line(4.0, 4.0),
            Seg::Line(0.0, 4.0),
            Seg::Move(1.0, 1.0),
            Seg::Line(1.0, 3.0),
            Seg::Line(3.0, 3.0),
            Seg::Line(3.0, 1.0),
        ];
        assert!((coverage(&ring, 8.0) - 12.0).abs() < 0.1);
    }

    #[test]
    fn a_curve_flattens_to_a_closed_shape_with_area() {
        let blob = [
            Seg::Move(0.0, 0.0),
            Seg::Curve(0.0, 2.0, 2.0, 2.0, 2.0, 0.0),
        ];
        // The area under this cubic is exactly 2.4.
        let area = coverage(&blob, 16.0);
        assert!((area - 2.4).abs() < 0.1, "{area}");
    }

    #[test]
    fn every_outline_has_ink() {
        for segs in [TREBLE, BASS, QUARTER, EIGHTH, SIXTEENTH, THIRTY_SECOND, SIXTY_FOURTH] {
            assert!(coverage(segs, 4.0) > 0.5);
        }
    }
}
