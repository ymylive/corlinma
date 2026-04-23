"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { motion } from "framer-motion";
import { Plus, UserSquare } from "lucide-react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useMotionVariants } from "@/lib/motion";
import { fetchAgents, type AgentCard } from "@/lib/mocks/characters";
import { CharacterCard, tiltForName } from "@/components/characters/character-card";
import { CharacterDrawer } from "@/components/characters/character-drawer";

/**
 * Character Cards page (B2-FE4).
 *
 * Card-deck metaphor. Each agent is a card; click to flip for details; click
 * Edit to open the right-side drawer. Data today is a local mock; when
 * B2-BE3 ships `GET /admin/agents` this fetcher swaps to a real call without
 * changing the component tree.
 */
export default function CharactersPage() {
  const variants = useMotionVariants();
  const query = useQuery<AgentCard[]>({
    queryKey: ["admin", "agents"],
    queryFn: fetchAgents,
  });

  // Which card (if any) is currently flipped to its front face.
  const [flippedName, setFlippedName] = React.useState<string | null>(null);
  // Which card the drawer is editing. `null` while closed. "__new__" means
  // create mode (header button pressed).
  const [drawerTarget, setDrawerTarget] = React.useState<string | null>(null);

  const byName = React.useMemo(() => {
    const map = new Map<string, AgentCard>();
    for (const c of query.data ?? []) map.set(c.name, c);
    return map;
  }, [query.data]);

  function onCardFlip(name: string) {
    setFlippedName((prev) => (prev === name ? null : name));
  }

  function onCardEdit(name: string) {
    setDrawerTarget(name);
  }

  function onDrawerOpenChange(open: boolean) {
    if (!open) {
      // Flip the card back when the drawer closes so the deck rests face-down.
      setDrawerTarget(null);
      setFlippedName(null);
    }
  }

  const drawerCard: AgentCard | null =
    drawerTarget && drawerTarget !== "__new__"
      ? (byName.get(drawerTarget) ?? null)
      : null;

  return (
    <>
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            角色卡 · Character Cards
          </h1>
          <p className="text-sm text-muted-foreground">
            A deck of agent characters. Click a card to peek at its tools and
            prompt — edit opens on the side.
          </p>
        </div>
        <Button
          size="sm"
          onClick={() => setDrawerTarget("__new__")}
          data-testid="character-new"
        >
          <Plus className="h-3.5 w-3.5" />
          New card
        </Button>
      </header>

      <section>
        {query.isPending ? (
          <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 6 }).map((_, i) => (
              <Skeleton key={i} className="h-[320px] w-full rounded-2xl" />
            ))}
          </div>
        ) : query.isError ? (
          <EmptyState
            icon={<UserSquare />}
            title="Could not load characters"
            description={(query.error as Error).message}
          />
        ) : !query.data || query.data.length === 0 ? (
          <EmptyState
            icon={<UserSquare />}
            title="No characters yet"
            description="Use + New card to scaffold your first character."
          />
        ) : (
          <motion.div
            initial="hidden"
            animate="visible"
            variants={variants.stagger}
            className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3"
          >
            {query.data.map((card) => (
              <motion.div key={card.name} variants={variants.listItem}>
                <CharacterCard
                  card={card}
                  flipped={flippedName === card.name}
                  rotateDeg={tiltForName(card.name)}
                  onFlip={() => onCardFlip(card.name)}
                  onEdit={() => onCardEdit(card.name)}
                />
              </motion.div>
            ))}
          </motion.div>
        )}
      </section>

      <CharacterDrawer
        open={drawerTarget !== null}
        onOpenChange={onDrawerOpenChange}
        card={drawerCard}
      />
    </>
  );
}
