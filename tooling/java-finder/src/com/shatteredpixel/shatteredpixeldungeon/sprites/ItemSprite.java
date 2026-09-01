/*
 * Headless stand-in for Shattered Pixel Dungeon's ItemSprite.
 * Copyright (C) 2026
 *
 * Shattered Pixel Dungeon is GPL-3.0 software and so is this stand-in:
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 */

package com.shatteredpixel.shatteredpixeldungeon.sprites;

import com.shatteredpixel.shatteredpixeldungeon.items.Heap;
import com.shatteredpixel.shatteredpixeldungeon.items.Item;
import com.watabou.noosa.Image;
import com.watabou.noosa.MovieClip;
import com.watabou.utils.PointF;

/**
 * A drawing-free replacement for {@code ItemSprite}, placed ahead of the game
 * JAR on the classpath.
 *
 * <p>Level generation is supposed to be sprite-free, but one path is not:
 * {@code Level.drop(item, cell)} builds a throwaway {@code Heap} with a
 * {@code new ItemSprite()} when the item it is handed is null or blocked by a
 * challenge, and {@code RingRoom.placeCenterDetail} hands it
 * {@code Level.findPrizeItem()}, which is null once the floor's prize items are
 * spent. The upstream constructor chain reaches
 * {@code TextureCache.get(Assets.Sprites.ITEMS)} and dies without a GL context
 * (roughly one seed in a hundred within nineteen floors).
 *
 * <p>The stand-in keeps the class's shape — it still extends
 * {@code MovieClip}, so a {@code Heap.sprite} still holds one — but its
 * constructors reach {@code MovieClip()}, which touches no texture, and every
 * method is a no-op. Nothing here reads or advances
 * {@code com.watabou.utils.Random}, and the discarded heap the game builds on
 * this path holds no item, so generation is unaffected.
 *
 * <p>The members typed in terms of {@code ItemSprite.Glowing} (the
 * {@code (int, Glowing)} constructor, {@code view(int, Glowing)} and
 * {@code glow}) are deliberately absent: the nested class lives in the JAR and
 * cannot be named from a source file that replaces its outer class. Only
 * display code calls them, so a generation run never resolves them; if one ever
 * does, it fails loudly with a {@code NoSuchMethodError} rather than quietly
 * drawing nothing.
 */
public class ItemSprite extends MovieClip {

	public static final int SIZE = 16;

	public Heap heap;

	public ItemSprite() {
		super();
	}

	public ItemSprite(Heap heap) {
		this();
		this.heap = heap;
	}

	public ItemSprite(Item item) {
		this();
	}

	public ItemSprite(int image) {
		this();
	}

	public void link() {
	}

	public void link(Heap heap) {
		this.heap = heap;
	}

	@Override
	public void revive() {
	}

	@Override
	public void copy(Image other) {
	}

	public void visible(boolean value) {
	}

	public PointF worldToCamera(int cell) {
		return new PointF();
	}

	public void place(int cell) {
	}

	public void drop() {
	}

	public void drop(int from) {
	}

	public ItemSprite view(Item item) {
		return this;
	}

	public ItemSprite view(Heap heap) {
		return this;
	}

	public void frame(int index) {
	}

	@Override
	public void kill() {
		super.kill();
	}

	@Override
	public void draw() {
	}

	@Override
	public synchronized void update() {
	}

	public static int pick(int index, int x, int y) {
		return 0;
	}
}
