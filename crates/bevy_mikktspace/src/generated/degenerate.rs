use crate::face_vert_to_index;
use crate::get_position;
use crate::Geometry;

use super::STSpace;
use super::STriInfo;
use super::TriangleFlags;

pub(crate) unsafe fn DegenPrologue(
    mut pTriInfos: *mut STriInfo,
    mut piTriList_out: *mut i32,
    iNrTrianglesIn: i32,
    iTotTris: i32,
) {
    // locate quads with only one good triangle
    let mut t: i32 = 0i32;
    while t < iTotTris - 1i32 {
        let iFO_a: i32 = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
        let iFO_b: i32 = (*pTriInfos.offset((t + 1i32) as isize)).iOrgFaceNumber;
        if iFO_a == iFO_b {
            let bIsDeg_a: bool = (*pTriInfos.offset(t as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            let bIsDeg_b: bool = (*pTriInfos.offset((t + 1i32) as isize))
                .iFlag
                .contains(TriangleFlags::DEGENERATE);
            // If exactly one is degenerate, mark both as QUAD_ONE_DEGENERATE_TRI, i.e. that the other triangle
            // (If both are degenerate, this)
            if bIsDeg_a ^ bIsDeg_b {
                (*pTriInfos.offset(t as isize))
                    .iFlag
                    .insert(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
                (*pTriInfos.offset((t + 1i32) as isize))
                    .iFlag
                    .insert(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
            }
            t += 2i32
        } else {
            t += 1
        }
    }

    // reorder list so all degen triangles are moved to the back
    // without reordering the good triangles
    // That is, a semi-stable partition, e.g. as described at
    // https://dlang.org/library/std/algorithm/sorting/partition.html
    // TODO: Use `Vec::retain` with a second vec here - not perfect,
    // but good enough and safe.
    // TODO: Consider using `sort_by_key` on Vec instead (which is stable) - it might be
    // technically slower, but it's much easier to reason about
    let mut iNextGoodTriangleSearchIndex = 1i32;
    t = 0i32;
    let mut bStillFindingGoodOnes = true;
    while t < iNrTrianglesIn && bStillFindingGoodOnes {
        let bIsGood: bool = !(*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::DEGENERATE);
        if bIsGood {
            if iNextGoodTriangleSearchIndex < t + 2i32 {
                iNextGoodTriangleSearchIndex = t + 2i32
            }
        } else {
            let mut bJustADegenerate: bool = true;
            while bJustADegenerate && iNextGoodTriangleSearchIndex < iTotTris {
                let bIsGood_0: bool = !(*pTriInfos.offset(iNextGoodTriangleSearchIndex as isize))
                    .iFlag
                    .contains(TriangleFlags::DEGENERATE);
                if bIsGood_0 {
                    bJustADegenerate = false
                } else {
                    iNextGoodTriangleSearchIndex += 1
                }
            }
            let t0 = t;
            let t1 = iNextGoodTriangleSearchIndex;
            iNextGoodTriangleSearchIndex += 1;
            debug_assert!(iNextGoodTriangleSearchIndex > (t + 1));
            // Swap t0 and t1
            if !bJustADegenerate {
                for i in 0..3i32 {
                    let index: i32 = *piTriList_out.offset((t0 * 3i32 + i) as isize);
                    *piTriList_out.offset((t0 * 3i32 + i) as isize) =
                        *piTriList_out.offset((t1 * 3i32 + i) as isize);
                    *piTriList_out.offset((t1 * 3i32 + i) as isize) = index;
                }
                let tri_info: STriInfo = *pTriInfos.offset(t0 as isize);
                *pTriInfos.offset(t0 as isize) = *pTriInfos.offset(t1 as isize);
                *pTriInfos.offset(t1 as isize) = tri_info
            } else {
                bStillFindingGoodOnes = false
            }
        }
        if bStillFindingGoodOnes {
            t += 1
        }
    }
    debug_assert!(iNrTrianglesIn == t);
    debug_assert!(bStillFindingGoodOnes);
}

pub(crate) unsafe fn DegenEpilogue(
    mut psTspace: *mut STSpace,
    mut pTriInfos: *mut STriInfo,
    mut piTriListIn: *mut i32,
    geometry: &impl Geometry,
    iNrTrianglesIn: i32,
    iTotTris: i32,
) {
    // For all degenerate triangles
    for t in iNrTrianglesIn..iTotTris {
        let bSkip: bool = (*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::QUAD_ONE_DEGENERATE_TRI);
        if !bSkip {
            for i in 0..3i32 {
                // For all vertices on that triangle
                let index1: i32 = *piTriListIn.offset((t * 3i32 + i) as isize);
                for j in 0..(3i32 * iNrTrianglesIn) {
                    let index2: i32 = *piTriListIn.offset(j as isize);
                    // If the vertex properties are the same as another non-degenerate vertex
                    if index1 == index2 {
                        let iTri: i32 = j / 3i32;
                        let iVert: i32 = j % 3i32;
                        let iSrcVert: i32 =
                            (*pTriInfos.offset(iTri as isize)).vert_num[iVert as usize] as i32;
                        let iSrcOffs: i32 = (*pTriInfos.offset(iTri as isize)).iTSpacesOffs;
                        let iDstVert: i32 =
                            (*pTriInfos.offset(t as isize)).vert_num[i as usize] as i32;
                        let iDstOffs: i32 = (*pTriInfos.offset(t as isize)).iTSpacesOffs;
                        // Set the tangent space of this vertex to the tangent space of that vertex
                        // TODO: This is absurd - doing a linear search through all vertices for each
                        // degenerate triangle?
                        *psTspace.offset((iDstOffs + iDstVert) as isize) =
                            *psTspace.offset((iSrcOffs + iSrcVert) as isize);
                        break;
                    }
                }
            }
        }
    }
    for t in 0..iNrTrianglesIn {
        // Handle quads with a single degenerate triangle by
        if (*pTriInfos.offset(t as isize))
            .iFlag
            .contains(TriangleFlags::QUAD_ONE_DEGENERATE_TRI)
        {
            let mut pV: *mut u8 = (*pTriInfos.offset(t as isize)).vert_num.as_mut_ptr();
            let mut iFlag: i32 = 1i32 << *pV.offset(0isize) as i32
                | 1i32 << *pV.offset(1isize) as i32
                | 1i32 << *pV.offset(2isize) as i32;
            let mut iMissingIndex: i32 = 0i32;
            if iFlag & 2i32 == 0i32 {
                iMissingIndex = 1i32
            } else if iFlag & 4i32 == 0i32 {
                iMissingIndex = 2i32
            } else if iFlag & 8i32 == 0i32 {
                iMissingIndex = 3i32
            }
            let iOrgF = (*pTriInfos.offset(t as isize)).iOrgFaceNumber;
            let vDstP = get_position(
                geometry,
                face_vert_to_index(iOrgF as usize, iMissingIndex as usize),
            );

            for i_0 in 0..3i32 {
                let iVert_0: i32 = *pV.offset(i_0 as isize) as i32;
                let vSrcP = get_position(
                    geometry,
                    face_vert_to_index(iOrgF as usize, iVert_0 as usize),
                );
                if vSrcP == vDstP {
                    let iOffs: i32 = (*pTriInfos.offset(t as isize)).iTSpacesOffs;
                    *psTspace.offset((iOffs + iMissingIndex) as isize) =
                        *psTspace.offset((iOffs + iVert_0) as isize);
                    break;
                }
            }
        }
    }
}
