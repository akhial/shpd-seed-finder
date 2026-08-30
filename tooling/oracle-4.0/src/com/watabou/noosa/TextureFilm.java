/*
 * Shattered Pixel Dungeon parity oracle
 * Copyright (C) 2026
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * HEADLESS STAND-IN for the GPL-3 upstream class com.watabou.noosa.TextureFilm
 * (Shattered Pixel Dungeon / Noosa, (C) Watabou, 00-Evan).  This copy is placed
 * ahead of the official JAR on the classpath so that the static initializer of
 * ItemSpriteSheet.Icons (new TextureFilm("sprites/item_icons.png", 8, 8)) does
 * not call TextureCache.get(), which needs a live libGDX graphics context.
 *
 * Only the geometry of the upstream class is reproduced.  Texture-backed
 * constructors resolve the texture size from the PNG IHDR header of the
 * classpath resource instead of uploading a GL texture.  No random numbers
 * are consumed anywhere in this class, so it is RNG-neutral by construction.
 */

package com.watabou.noosa;

import com.watabou.gltextures.SmartTexture;
import com.watabou.utils.RectF;

import java.io.DataInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.HashMap;

public class TextureFilm {

	private static final RectF FULL = new RectF(0, 0, 1, 1);

	private int texWidth;
	private int texHeight;

	protected HashMap<Object, RectF> frames = new HashMap<Object, RectF>();

	public TextureFilm(Object tx) {
		int[] size = sizeOf(tx);
		texWidth = size[0];
		texHeight = size[1];
		add(null, FULL);
	}

	public TextureFilm(SmartTexture texture, int width) {
		this(texture, width, texture.height);
	}

	public TextureFilm(Object tx, int width, int height) {
		int[] size = sizeOf(tx);
		texWidth = size[0];
		texHeight = size[1];
		grid(width, height);
	}

	public TextureFilm(TextureFilm atlas, Object key, int width, int height) {
		texWidth = atlas.texWidth;
		texHeight = atlas.texHeight;

		RectF patch = atlas.get(key);

		float uw = (float) width / texWidth;
		float vh = (float) height / texHeight;

		int cols = (int) (width(patch) / width);
		int rows = (int) (height(patch) / height);

		for (int i = 0; i < rows; i++) {
			for (int j = 0; j < cols; j++) {
				RectF rect = new RectF(j * uw, i * vh, (j + 1) * uw, (i + 1) * vh);
				rect.shift(patch.left, patch.top);
				add(i * cols + j, rect);
			}
		}
	}

	public TextureFilm(int txWidth, int txHeight, int width, int height) {
		texWidth = txWidth;
		texHeight = txHeight;
		grid(width, height);
	}

	private void grid(int width, int height) {
		float uw = (float) width / texWidth;
		float vh = (float) height / texHeight;

		int cols = texWidth / width;
		int rows = texHeight / height;

		for (int i = 0; i < rows; i++) {
			for (int j = 0; j < cols; j++) {
				RectF rect = new RectF(j * uw, i * vh, (j + 1) * uw, (i + 1) * vh);
				add(i * cols + j, rect);
			}
		}
	}

	/** Resolves a texture size without touching libGDX. */
	private static int[] sizeOf(Object tx) {
		if (tx instanceof SmartTexture) {
			SmartTexture texture = (SmartTexture) tx;
			return new int[]{texture.width, texture.height};
		}
		if (tx instanceof String) {
			return pngSize((String) tx);
		}
		throw new IllegalStateException("headless TextureFilm cannot size texture " + tx);
	}

	private static int[] pngSize(String resource) {
		String path = resource.startsWith("/") ? resource : "/" + resource;
		InputStream stream = TextureFilm.class.getResourceAsStream(path);
		if (stream == null) {
			throw new IllegalStateException("headless TextureFilm: missing classpath resource " + resource);
		}
		try (DataInputStream input = new DataInputStream(stream)) {
			byte[] header = new byte[24];
			input.readFully(header);
			// PNG signature, then the IHDR chunk: length(4) "IHDR"(4) width(4) height(4).
			if ((header[0] & 0xff) != 0x89 || header[1] != 'P' || header[2] != 'N' || header[3] != 'G'
					|| header[12] != 'I' || header[13] != 'H' || header[14] != 'D' || header[15] != 'R') {
				throw new IllegalStateException("headless TextureFilm: " + resource + " is not a PNG");
			}
			int width = ((header[16] & 0xff) << 24) | ((header[17] & 0xff) << 16)
					| ((header[18] & 0xff) << 8) | (header[19] & 0xff);
			int height = ((header[20] & 0xff) << 24) | ((header[21] & 0xff) << 16)
					| ((header[22] & 0xff) << 8) | (header[23] & 0xff);
			return new int[]{width, height};
		} catch (IOException error) {
			throw new IllegalStateException("headless TextureFilm: cannot read " + resource, error);
		}
	}

	public void add(Object id, RectF rect) {
		frames.put(id, rect);
	}

	public void add(Object id, float left, float top, float right, float bottom) {
		frames.put(id, new RectF(left / texWidth, top / texHeight, right / texWidth, bottom / texHeight));
	}

	public RectF get(Object id) {
		return frames.get(id);
	}

	public float width(Object id) {
		return width(get(id));
	}

	public float width(RectF frame) {
		return frame.width() * texWidth;
	}

	public float height(Object id) {
		return height(get(id));
	}

	public float height(RectF frame) {
		return frame.height() * texHeight;
	}
}
