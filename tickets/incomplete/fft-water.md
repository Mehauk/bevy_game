Scenario 2: Dynamic weather (Calm

        
↔
↔

      

Storm transitions in real time)

If you just bake one Storm TextureArray and scale fft_amplitude down to 0.1 for calm water, it looks slightly off: in real oceans, calm water has short, crisp ripples, whereas scaling down a storm bake gives you tiny, slow, long swells.

Instead of baking 3–4 full presets and cross-fading between them, the cleanest trick (used in many commercial games) is to bake 2 complementary frequency bands:

    Swell Bake (tile_size = 110m–140m): Long, heavy, rolling ocean swells.

    Chop Bake (tile_size = 30m–40m): Smaller, steeper, faster surface waves and chop.

Then you just expose two sliders (swell_strength and chop_strength) and add them together in vertex() and fragment():

    Calm: 0.1 * Swell + 0.25 * Chop (low detail normal & zero whitecaps)

    Moderate: 0.5 * Swell + 0.7 * Chop

    Storm: 1.2 * Swell + 1.0 * Chop (high whitecap foam & strong detail normals)

Major bonus: Combining two non-matching tile sizes (like 120m and 37m) breaks up visible repeating patterns across large open water.