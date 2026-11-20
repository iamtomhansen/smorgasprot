# SmorgåsProt
Protein interface analysis tool. Provides interactive visualization of interactions at a protein's
surface.

# Math explanation
Treats protein surfaces as 2-manifolds which are projected onto a SVD plane. Each atom becomes a
node in the graph, where edge length corresponds to 3D interatomic distance. A set of energy
functions works to preserve those distances while flattening nodes into 2D space for presentation
and analysis.

# Human explanation
Opens a protein complex like a sandwich.